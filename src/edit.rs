//! Command processor

use std::fmt;

use crate::error::{ReadlineError, Signal};
use crate::history::{History, SearchDirection};
use crate::keymap::{CharSearch, Cmd, InputState, Movement, Refresher, RepeatCount, Word};
use crate::kill_ring::KillRing;
use crate::layout::{cwidh, Layout, Position, Unit};
use crate::line_buffer::{DeleteListener, Direction, LineBuffer, NoListener, WordAction, MAX_LINE};
use crate::tty::{RawReader, Renderer, Term, Terminal};
use crate::undo::Changeset;
use crate::Result;

/// Represent the state during line editing.
/// Implement rendering.
pub(crate) struct State<'out, 'prompt> {
    pub out: &'out mut <Terminal as Term>::Writer,
    prompt: &'prompt str,  // Prompt to display (rl_prompt)
    prompt_size: Position, // Prompt Unicode/visible width and height
    pub line: LineBuffer,  // Edited line buffer
    pub layout: Layout,
    saved_line_for_history: LineBuffer, // Current edited line before history browsing
    byte_buffer: [u8; 4],
    pub changes: Changeset, // changes to line, for undo/redo
    history: &'out dyn History,
    pub history_index: usize, // The history index we are currently editing
}

impl<'out, 'prompt> State<'out, 'prompt> {
    pub fn new(
        out: &'out mut <Terminal as Term>::Writer,
        prompt: &'prompt str,
        history: &'out dyn History,
    ) -> Self {
        let prompt_size = out.calculate_position(prompt, Position::default());
        let gcm = out.grapheme_cluster_mode();
        Self {
            out,
            prompt,
            prompt_size,
            line: LineBuffer::with_capacity(MAX_LINE),
            layout: Layout::new(gcm),
            saved_line_for_history: LineBuffer::with_capacity(MAX_LINE),
            byte_buffer: [0; 4],
            changes: Changeset::new(),
            history,
            history_index: history.len(),
        }
    }

    pub fn next_cmd<R: RawReader>(
        &mut self,
        input_state: &mut InputState,
        rdr: &mut R,
        single_esc_abort: bool,
    ) -> Result<Cmd> {
        loop {
            let rc = input_state.next_cmd(rdr, self, single_esc_abort);
            if let Err(ReadlineError::Signal(signal)) = rc {
                match signal {
                    #[cfg(unix)]
                    Signal::Interrupt => {
                        return Ok(Cmd::Interrupt);
                    }
                    Signal::Resize => {
                        let old_cols = self.out.get_columns();
                        self.out.update_size();
                        let new_cols = self.out.get_columns();
                        if new_cols != old_cols
                            && (self.layout.end.row > 0 || self.layout.end.col >= new_cols)
                        {
                            self.prompt_size = self
                                .out
                                .calculate_position(self.prompt, Position::default());
                            self.refresh_line()?;
                        }
                        continue;
                    }
                }
            }
            return rc;
        }
    }

    pub fn backup(&mut self) {
        self.saved_line_for_history
            .update(self.line.as_str(), self.line.pos(), &mut NoListener);
    }

    pub fn restore(&mut self) {
        self.line.update(
            self.saved_line_for_history.as_str(),
            self.saved_line_for_history.pos(),
            &mut self.changes,
        );
    }

    pub fn move_cursor(&mut self) -> Result<()> {
        // calculate the desired position of the cursor
        let cursor = self
            .out
            .calculate_position(&self.line[..self.line.pos()], self.prompt_size);
        if self.layout.cursor == cursor {
            return Ok(());
        }
        self.out.move_cursor(self.layout.cursor, cursor)?;
        self.layout.prompt_size = self.prompt_size;
        self.layout.cursor = cursor;
        debug_assert!(self.layout.prompt_size <= self.layout.cursor);
        debug_assert!(self.layout.cursor <= self.layout.end);
        Ok(())
    }

    pub fn move_cursor_to_end(&mut self) -> Result<()> {
        if self.layout.cursor == self.layout.end {
            return Ok(());
        }
        self.out.move_cursor(self.layout.cursor, self.layout.end)?;
        self.layout.cursor = self.layout.end;
        Ok(())
    }

    fn refresh(&mut self, prompt: &str, prompt_size: Position, default_prompt: bool) -> Result<()> {
        let new_layout = self
            .out
            .compute_layout(prompt_size, default_prompt, &self.line);
        self.out
            .refresh_line(prompt, &self.line, Some(&self.layout), &new_layout)?;
        self.layout = new_layout;
        Ok(())
    }

    pub fn is_default_prompt(&self) -> bool {
        self.layout.default_prompt
    }
}

impl Refresher for State<'_, '_> {
    fn refresh_line(&mut self) -> Result<()> {
        self.refresh(self.prompt, self.prompt_size, true)
    }

    fn refresh_prompt_and_line(&mut self, prompt: &str) -> Result<()> {
        let prompt_size = self.out.calculate_position(prompt, Position::default());
        self.refresh(prompt, prompt_size, false)
    }

    fn line(&self) -> &str {
        self.line.as_str()
    }
}

impl fmt::Debug for State<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("prompt", &self.prompt)
            .field("prompt_size", &self.prompt_size)
            .field("buf", &self.line)
            .field("cols", &self.out.get_columns())
            .field("layout", &self.layout)
            .field("saved_line_for_history", &self.saved_line_for_history)
            .finish()
    }
}

impl State<'_, '_> {
    pub fn clear_screen(&mut self) -> Result<()> {
        self.out.clear_screen()?;
        self.layout.cursor = Position::default();
        self.layout.end = Position::default();
        Ok(())
    }

    /// Insert the character `ch` at cursor current position.
    pub fn edit_insert(&mut self, ch: char, n: RepeatCount) -> Result<()> {
        let push = self.line.insert(ch, n, &mut self.changes);
        if push {
            let width = cwidh(ch);
            if n == 1
                && width != 0 // Ctrl-V + \t or \n ...
                && self.layout.cursor.col + width < self.out.get_columns()
            {
                // Avoid a full update of the line in the trivial case.
                self.layout.cursor.col += width;
                self.layout.end.col += width;
                debug_assert!(self.layout.prompt_size <= self.layout.cursor);
                debug_assert!(self.layout.cursor <= self.layout.end);
                let bits = ch.encode_utf8(&mut self.byte_buffer);
                self.out.write_and_flush(bits)
            } else {
                self.refresh_line()
            }
        } else {
            self.refresh_line()
        }
    }

    // Yank/paste `text` at current position.
    pub fn edit_yank(&mut self, text: &str, n: RepeatCount) -> Result<()> {
        if self.line.yank(text, n, &mut self.changes).is_some() {
            self.refresh_line()
        } else {
            Ok(())
        }
    }

    // Delete previously yanked text and yank/paste `text` at current position.
    pub fn edit_yank_pop(&mut self, yank_size: usize, text: &str) -> Result<()> {
        self.changes.begin();
        let result = if self
            .line
            .yank_pop(yank_size, text, &mut self.changes)
            .is_some()
        {
            self.refresh_line()
        } else {
            Ok(())
        };
        self.changes.end();
        result
    }

    /// Move cursor on the left.
    pub fn edit_move_backward(&mut self, n: RepeatCount) -> Result<()> {
        if self.line.move_backward(n) {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    /// Move cursor on the right.
    pub fn edit_move_forward(&mut self, n: RepeatCount) -> Result<()> {
        if self.line.move_forward(n) {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    /// Move cursor to the start of the line.
    pub fn edit_move_home(&mut self) -> Result<()> {
        if self.line.move_home() {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    /// Move cursor to the end of the line.
    pub fn edit_move_end(&mut self) -> Result<()> {
        if self.line.move_end() {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    /// Move cursor to the end of the buffer.
    pub fn edit_move_buffer_end(&mut self) -> Result<()> {
        if self.line.move_buffer_end() {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    pub fn edit_kill(&mut self, mvt: &Movement, kill_ring: &mut KillRing) -> Result<()> {
        struct Proxy<'p> {
            changes: &'p mut Changeset,
            kill_ring: &'p mut KillRing,
            layout: &'p Layout, // current layout (before kill)
            pos: usize,         // current cursor (byte) position (before kill)
            end: usize,         // end (before kill)
            cursor_shift: Unit, // cursor shift (columns) after kill
            end_shift: Unit,    // end of line shift (columns) after kill
            trivial: bool,      // true if a partial screen update can be done
        }
        impl DeleteListener for Proxy<'_> {
            fn start_killing(&mut self) {
                self.kill_ring.start_killing();
            }

            fn delete(&mut self, idx: usize, string: &str, dir: Direction) {
                self.changes.delete(idx, string);
                self.kill_ring.delete(idx, string, dir);
                if self.trivial {
                    if dir == Direction::Backward {
                        if self.pos == self.end {
                            // backward from eol
                            let width = self.layout.width(string);
                            self.cursor_shift += width;
                            self.end_shift += width;
                        } else {
                            self.trivial = false;
                        }
                    } else if idx == self.pos && self.pos + string.len() == self.end {
                        // forward to eol
                        self.end_shift += self.layout.width(string);
                    } else {
                        self.trivial = false;
                    }
                }
            }

            fn stop_killing(&mut self) {
                self.kill_ring.stop_killing();
            }
        }
        let mut proxy = Proxy {
            changes: &mut self.changes,
            kill_ring,
            layout: &self.layout,
            pos: self.line.pos(),
            end: self.line.len(),
            cursor_shift: 0,
            end_shift: 0,
            trivial: self.layout.cursor.row == self.layout.end.row,
        };
        if self.line.kill(mvt, &mut proxy) {
            let (trivial, cursor_shift, end_shift) =
                (proxy.trivial, proxy.cursor_shift, proxy.end_shift);
            if trivial && cursor_shift <= self.layout.cursor.col && end_shift <= self.layout.end.col
            {
                // Avoid a full update of the line in the trivial case.
                debug_assert!(self.line.is_cursor_at_end());
                if cursor_shift != 0 {
                    let old = self.layout.cursor;
                    self.layout.cursor.col -= cursor_shift;
                    self.out.move_cursor(old, self.layout.cursor)?;
                }
                self.layout.end.col -= end_shift;
                debug_assert_eq!(self.layout.cursor, self.layout.end);
                self.out.clear_to_eol()
            } else {
                self.refresh_line()
            }
        } else {
            Ok(())
        }
    }

    /// Exchange the char before cursor with the character at cursor.
    pub fn edit_transpose_chars(&mut self) -> Result<()> {
        self.changes.begin();
        let succeed = self.line.transpose_chars(&mut self.changes);
        self.changes.end();
        if succeed {
            self.refresh_line()
        } else {
            Ok(())
        }
    }

    pub fn edit_move_to_prev_word(&mut self, word_def: Word, n: RepeatCount) -> Result<()> {
        if self.line.move_to_prev_word(word_def, n) {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    pub fn edit_move_to_next_word(&mut self, word_def: Word, n: RepeatCount) -> Result<()> {
        if self.line.move_to_next_word(word_def, n) {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    /// Moves the cursor to the same column in the line above
    pub fn edit_move_line_up(&mut self, n: RepeatCount) -> Result<bool> {
        if self.line.move_to_line_up(n, &self.layout) {
            self.move_cursor()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Moves the cursor to the same column in the line below
    pub fn edit_move_line_down(&mut self, n: RepeatCount) -> Result<bool> {
        if self.line.move_to_line_down(n, &self.layout) {
            self.move_cursor()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn edit_move_to(&mut self, cs: CharSearch, n: RepeatCount) -> Result<()> {
        if self.line.move_to(cs, n) {
            self.move_cursor()
        } else {
            Ok(())
        }
    }

    pub fn edit_word(&mut self, a: WordAction) -> Result<()> {
        self.changes.begin();
        let succeed = self.line.edit_word(a, &mut self.changes);
        self.changes.end();
        if succeed {
            self.refresh_line()
        } else {
            Ok(())
        }
    }

    pub fn edit_transpose_words(&mut self, n: RepeatCount) -> Result<()> {
        self.changes.begin();
        let succeed = self.line.transpose_words(n, &mut self.changes);
        self.changes.end();
        if succeed {
            self.refresh_line()
        } else {
            Ok(())
        }
    }

    /// Substitute the currently edited line with the next or previous history
    /// entry.
    pub fn edit_history_next(&mut self, prev: bool) -> Result<()> {
        let history = self.history;
        if history.is_empty() {
            return Ok(());
        }
        if self.history_index == history.len() {
            if prev {
                // Save the current edited line before overwriting it
                self.backup();
            } else {
                return Ok(());
            }
        } else if self.history_index == 0 && prev {
            return Ok(());
        }
        let (idx, dir) = if prev {
            (self.history_index - 1, SearchDirection::Reverse)
        } else {
            self.history_index += 1;
            (self.history_index, SearchDirection::Forward)
        };
        if idx < history.len() {
            if let Some(r) = history.get(idx, dir)? {
                let buf = r.entry;
                self.history_index = r.idx;
                self.changes.begin();
                self.line.update(&buf, buf.len(), &mut self.changes);
                self.changes.end();
            } else {
                return Ok(());
            }
        } else {
            // Restore current edited line
            self.restore();
        }
        self.refresh_line()
    }

    /// Substitute the currently edited line with the first/last history entry.
    pub fn edit_history(&mut self, first: bool) -> Result<()> {
        let history = self.history;
        if history.is_empty() {
            return Ok(());
        }
        if self.history_index == history.len() {
            if first {
                // Save the current edited line before overwriting it
                self.backup();
            } else {
                return Ok(());
            }
        } else if self.history_index == 0 && first {
            return Ok(());
        }
        if first {
            if let Some(r) = history.get(0, SearchDirection::Forward)? {
                let buf = r.entry;
                self.history_index = r.idx;
                self.changes.begin();
                self.line.update(&buf, buf.len(), &mut self.changes);
                self.changes.end();
            } else {
                return Ok(());
            }
        } else {
            self.history_index = history.len();
            // Restore current edited line
            self.restore();
        }
        self.refresh_line()
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct Ed {
    out: crate::tty::Sink,
    pub history: crate::history::DefaultHistory,
}

#[cfg(test)]
impl Ed {
    pub fn init_state<'out>(&'out mut self, line: &str, pos: usize) -> State<'out, 'static> {
        State {
            out: &mut self.out,
            prompt: "",
            prompt_size: Position::default(),
            line: LineBuffer::init(line, pos),
            layout: Layout::default(),
            saved_line_for_history: LineBuffer::with_capacity(100),
            byte_buffer: [0; 4],
            changes: Changeset::new(),
            history: &self.history,
            history_index: self.history.len(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::Ed;
    use crate::history::History as _;

    #[test]
    fn edit_history_next() {
        let mut ed = Ed::default();
        ed.history.add("line0").unwrap();
        ed.history.add("line1").unwrap();
        let history_len = ed.history.len();
        let line = "current edited line";
        let mut s = ed.init_state(line, 6);
        s.history_index = history_len;

        for _ in 0..2 {
            s.edit_history_next(false).unwrap();
            assert_eq!(line, s.line.as_str());
        }

        s.edit_history_next(true).unwrap();
        assert_eq!(line, s.saved_line_for_history.as_str());
        assert_eq!(1, s.history_index);
        assert_eq!("line1", s.line.as_str());

        for _ in 0..2 {
            s.edit_history_next(true).unwrap();
            assert_eq!(line, s.saved_line_for_history.as_str());
            assert_eq!(0, s.history_index);
            assert_eq!("line0", s.line.as_str());
        }

        s.edit_history_next(false).unwrap();
        assert_eq!(line, s.saved_line_for_history.as_str());
        assert_eq!(1, s.history_index);
        assert_eq!("line1", s.line.as_str());

        s.edit_history_next(false).unwrap();
        // assert_eq!(line, s.saved_line_for_history);
        assert_eq!(2, s.history_index);
        assert_eq!(line, s.line.as_str());
    }
}
