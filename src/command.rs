use crate::edit::State;
use crate::error;
use crate::keymap::{Cmd, Movement, Refresher as _};
use crate::kill_ring::KillRing;
use crate::line_buffer::WordAction;
use crate::Result;

pub(crate) enum Status {
    Proceed,
    Submit,
}

pub(crate) fn execute(cmd: Cmd, s: &mut State<'_, '_>, kill_ring: &mut KillRing) -> Result<Status> {
    use Status::{Proceed, Submit};

    if matches!(cmd, Cmd::EndOfFile | Cmd::AcceptLine) && !s.is_default_prompt() {
        // Force a refresh with the default prompt to leave the
        // previous line as the user typed it after a newline.
        s.refresh_line()?;
    }
    match cmd {
        Cmd::SelfInsert(n, c) => {
            s.edit_insert(c, n)?;
        }
        Cmd::Insert(n, text) => {
            s.edit_yank(&text, n)?;
        }
        Cmd::Move(Movement::BeginningOfLine) => {
            // Move to the beginning of line.
            s.edit_move_home()?;
        }
        Cmd::Move(Movement::BackwardChar(n)) => {
            // Move back a character.
            s.edit_move_backward(n)?;
        }
        Cmd::EndOfFile => {
            if s.line.is_empty() {
                return Err(error::ReadlineError::Eof);
            }
        }
        Cmd::Move(Movement::EndOfLine) => {
            // Move to the end of line.
            s.edit_move_end()?;
        }
        Cmd::Move(Movement::ForwardChar(n)) => {
            // Move forward a character.
            s.edit_move_forward(n)?;
        }
        Cmd::ClearScreen => {
            // Clear the screen leaving the current line at the top of the
            // screen.
            s.clear_screen()?;
            s.refresh_line()?;
        }
        Cmd::NextHistory => {
            // Fetch the next command from the history list.
            s.edit_history_next(false)?;
        }
        Cmd::PreviousHistory => {
            // Fetch the previous command from the history list.
            s.edit_history_next(true)?;
        }
        Cmd::LineUpOrPreviousHistory(n) => {
            if !s.edit_move_line_up(n)? {
                s.edit_history_next(true)?;
            }
        }
        Cmd::LineDownOrNextHistory(n) => {
            if !s.edit_move_line_down(n)? {
                s.edit_history_next(false)?;
            }
        }
        Cmd::TransposeChars => {
            // Exchange the char before cursor with the character at cursor.
            s.edit_transpose_chars()?;
        }
        Cmd::Yank(n) => {
            // retrieve (yank) last item killed
            if let Some(text) = kill_ring.yank() {
                s.edit_yank(text, n)?;
            }
        }
        Cmd::AcceptLine => {
            return Ok(Submit);
        }
        Cmd::BeginningOfHistory => {
            // move to first entry in history
            s.edit_history(true)?;
        }
        Cmd::EndOfHistory => {
            // move to last entry in history
            s.edit_history(false)?;
        }
        Cmd::Move(Movement::BackwardWord(n, word_def)) => {
            // move backwards one word
            s.edit_move_to_prev_word(word_def, n)?;
        }
        Cmd::CapitalizeWord => {
            // capitalize word after point
            s.edit_word(WordAction::Capitalize)?;
        }
        Cmd::Kill(ref mvt) => {
            s.edit_kill(mvt, kill_ring)?;
        }
        Cmd::Move(Movement::ForwardWord(n, word_def)) => {
            // move forwards one word
            s.edit_move_to_next_word(word_def, n)?;
        }
        Cmd::DowncaseWord => {
            // lowercase word after point
            s.edit_word(WordAction::Lowercase)?;
        }
        Cmd::TransposeWords(n) => {
            // transpose words
            s.edit_transpose_words(n)?;
        }
        Cmd::UpcaseWord => {
            // uppercase word after point
            s.edit_word(WordAction::Uppercase)?;
        }
        Cmd::YankPop => {
            // yank-pop
            if let Some((yank_size, text)) = kill_ring.yank_pop() {
                s.edit_yank_pop(yank_size, text)?;
            }
        }
        Cmd::Move(Movement::ViCharSearch(n, cs)) => s.edit_move_to(cs, n)?,
        Cmd::Undo(n) => {
            if s.changes.undo(&mut s.line, n) {
                s.refresh_line()?;
            }
        }
        Cmd::Interrupt => {
            // Move to end, in case cursor was in the middle of the
            // line, so that next thing application prints goes after
            // the input
            s.move_cursor_to_end()?;
            return Err(error::ReadlineError::Interrupted);
        }
        _ => {
            // Ignore the character typed.
        }
    }
    Ok(Proceed)
}
