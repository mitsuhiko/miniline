//! Readline for Rust, without dependencies.
//!
//! `miniline` is a trimmed down, dependency free fork of
//! [rustyline](https://github.com/kkawakam/rustyline) by Katsu Kawakami and
//! the Rustyline authors, which itself is based on [Antirez's
//! Linenoise](https://github.com/antirez/linenoise).  It keeps rustyline's
//! API for the supported subset: Emacs style line editing, an in-memory
//! history, kill ring and undo.  Completion, hints, highlighting,
//! validation, Vi mode, custom key bindings and file based history are not
//! supported.
//!
//! # Example
//!
//! Usage
//!
//! ```
//! let mut rl = miniline::DefaultEditor::new()?;
//! let readline = rl.readline(">> ");
//! match readline {
//!     Ok(line) => println!("Line: {:?}", line),
//!     Err(_) => println!("No input"),
//! }
//! # Ok::<(), miniline::error::ReadlineError>(())
//! ```
//!
//! # Features
//!
//! * `unicode`: use the `unicode-segmentation` and `unicode-width` crates
//!   for accurate grapheme clustering and display widths.  Without this
//!   feature a compact built-in approximation is used.  The two crates can
//!   also be enabled individually via the `unicode-segmentation` and
//!   `unicode-width` features.
#![warn(missing_docs)]
// On platforms without terminal support only the plain line reading is used.
#![cfg_attr(
    any(target_arch = "wasm32", not(any(unix, windows))),
    allow(dead_code, unused_imports)
)]

mod command;
pub mod config;
mod edit;
pub mod error;
pub mod history;
mod keymap;
mod keys;
mod kill_ring;
mod layout;
mod line_buffer;
mod tty;
mod undo;
mod unicode;

use std::fmt;
use std::io::{self, BufRead, Write};
use std::marker::PhantomData;

pub use crate::config::{Config, HistoryDuplicates};
use crate::edit::State;
use crate::error::ReadlineError;
use crate::history::{DefaultHistory, History, SearchDirection};
use crate::keymap::{Cmd, InputState, Movement, Refresher as _};
use crate::kill_ring::KillRing;
#[cfg(unix)]
use crate::tty::Renderer as _;
use crate::tty::{RawMode as _, RawReader, Term, Terminal};

/// The error type for I/O and Linux Syscalls (Errno)
pub type Result<T> = std::result::Result<T, ReadlineError>;

/// Incremental search
fn reverse_incremental_search<R: RawReader, I: History>(
    rdr: &mut R,
    s: &mut State<'_, '_>,
    input_state: &mut InputState,
    history: &I,
) -> Result<Option<Cmd>> {
    if history.is_empty() {
        return Ok(None);
    }
    let mark = s.changes.begin();
    // Save the current edited line (and cursor position) before overwriting it
    let backup = s.line.as_str().to_owned();
    let backup_pos = s.line.pos();

    let mut search_buf = String::new();
    let mut history_idx = history.len() - 1;
    let mut direction = SearchDirection::Reverse;
    let mut success = true;

    let mut cmd;
    // Display the reverse-i-search prompt and process chars
    loop {
        let prompt = if success {
            format!("(reverse-i-search)`{search_buf}': ")
        } else {
            format!("(failed reverse-i-search)`{search_buf}': ")
        };
        s.refresh_prompt_and_line(&prompt)?;

        cmd = s.next_cmd(input_state, rdr, true)?;
        if let Cmd::SelfInsert(_, c) = cmd {
            search_buf.push(c);
        } else {
            match cmd {
                Cmd::Kill(Movement::BackwardChar(_)) => {
                    search_buf.pop();
                    continue;
                }
                Cmd::ReverseSearchHistory => {
                    direction = SearchDirection::Reverse;
                    if history_idx > 0 {
                        history_idx -= 1;
                    } else {
                        success = false;
                        continue;
                    }
                }
                Cmd::ForwardSearchHistory => {
                    direction = SearchDirection::Forward;
                    if history_idx < history.len() - 1 {
                        history_idx += 1;
                    } else {
                        success = false;
                        continue;
                    }
                }
                Cmd::Abort => {
                    // Restore current edited line (before search)
                    s.line.update(&backup, backup_pos, &mut s.changes);
                    s.refresh_line()?;
                    s.changes.truncate(mark);
                    return Ok(None);
                }
                Cmd::Move(_) => {
                    s.refresh_line()?; // restore prompt
                    break;
                }
                _ => break,
            }
        }
        success = match history.search(&search_buf, history_idx, direction)? {
            Some(sr) => {
                history_idx = sr.idx;
                s.line.update(&sr.entry, sr.pos, &mut s.changes);
                true
            }
            _ => false,
        };
    }
    s.changes.end();
    Ok(Some(cmd))
}

struct Guard<'m>(&'m tty::Mode);

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let Guard(mode) = *self;
        let _ = mode.disable_raw_mode();
    }
}

// Helper to handle backspace characters in a direct input
fn apply_backspace_direct(input: &str) -> String {
    // Setup the output buffer
    // No '\b' in the input in the common case, so set the capacity to the input
    // length
    let mut out = String::with_capacity(input.len());

    // Keep track of the size of each grapheme from the input
    // As many graphemes as input bytes in the common case
    let mut grapheme_sizes: Vec<u8> = Vec::with_capacity(input.len());

    for g in unicode::graphemes(input) {
        if g == "\u{0008}" {
            // backspace char
            if let Some(n) = grapheme_sizes.pop() {
                // Remove the last grapheme
                out.truncate(out.len() - n as usize);
            }
        } else {
            out.push_str(g);
            #[allow(clippy::cast_possible_truncation)]
            grapheme_sizes.push(g.len() as u8);
        }
    }

    out
}

fn readline_direct(mut reader: impl BufRead) -> Result<String> {
    let mut input = String::new();

    if reader.read_line(&mut input)? == 0 {
        return Err(ReadlineError::Eof);
    }
    // Remove trailing newline
    if input.ends_with('\n') {
        input.pop();
        if input.ends_with('\r') {
            input.pop();
        }
    }

    Ok(apply_backspace_direct(&input))
}

/// Syntax specific helper.
///
/// miniline does not support completion, hints, highlighting or validation.
/// This trait only exists so that [`Editor`] has the same shape as in
/// rustyline.  It is implemented for `()`.
pub trait Helper {}

impl Helper for () {}

/// Line editor
#[must_use]
pub struct Editor<H: Helper, I: History> {
    term: Terminal,
    buffer: Option<<Terminal as Term>::Buffer>,
    history: I,
    kill_ring: KillRing,
    config: Config,
    _helper: PhantomData<H>,
}

/// Default editor with default helper and `DefaultHistory`
pub type DefaultEditor = Editor<(), DefaultHistory>;

impl<H: Helper> Editor<H, DefaultHistory> {
    /// Create an editor with the default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(Config::default())
    }

    /// Create an editor with a specific configuration.
    pub fn with_config(config: Config) -> Result<Self> {
        let history = DefaultHistory::with_config(&config);
        Self::with_history(config, history)
    }
}

impl<H: Helper, I: History> Editor<H, I> {
    /// Create an editor with a custom history impl.
    pub fn with_history(config: Config, history: I) -> Result<Self> {
        let term = Terminal::new(&config)?;
        Ok(Self {
            term,
            buffer: None,
            history,
            kill_ring: KillRing::new(60),
            config,
            _helper: PhantomData,
        })
    }

    /// This method will read a line from STDIN and will display a `prompt`.
    ///
    /// `prompt` should not be styled (in case the terminal doesn't support
    /// ANSI) directly.
    ///
    /// It uses terminal-style interaction if `stdin` is connected to a
    /// terminal.
    /// Otherwise (e.g., if `stdin` is a pipe or the terminal is not supported),
    /// it uses file-style interaction.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn readline(&mut self, prompt: &str) -> Result<String> {
        self.readline_with(prompt, None)
    }

    /// This function behaves in the exact same manner as [`Editor::readline`],
    /// except that it pre-populates the input area.
    ///
    /// The text that resides in the input area is given as a 2-tuple.
    /// The string on the left of the tuple is what will appear to the left of
    /// the cursor and the string on the right is what will appear to the
    /// right of the cursor.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn readline_with_initial(&mut self, prompt: &str, initial: (&str, &str)) -> Result<String> {
        self.readline_with(prompt, Some(initial))
    }

    fn readline_with(&mut self, prompt: &str, initial: Option<(&str, &str)>) -> Result<String> {
        if self.term.is_unsupported() {
            // Write prompt and flush it to stdout
            let mut stdout = io::stdout();
            stdout.write_all(prompt.as_bytes())?;
            stdout.flush()?;

            readline_direct(io::stdin().lock())
        } else if self.term.is_input_tty() {
            let (original_mode, term_key_map) = self.term.enable_raw_mode(&self.config)?;
            let guard = Guard(&original_mode);
            let user_input = self.readline_edit(prompt, initial, &original_mode, term_key_map);
            if self.config.auto_add_history() {
                if let Ok(ref line) = user_input {
                    self.add_history_entry(line.as_str())?;
                }
            }
            drop(guard); // disable_raw_mode(original_mode)?;
            self.term.writeln()?;
            user_input
        } else {
            // Not a tty: read from file / pipe.
            readline_direct(io::stdin().lock())
        }
    }

    /// Handles reading and editing the readline buffer.
    /// It will also handle special inputs in an appropriate fashion
    /// (e.g., C-c will exit readline)
    fn readline_edit(
        &mut self,
        prompt: &str,
        initial: Option<(&str, &str)>,
        original_mode: &tty::Mode,
        term_key_map: tty::KeyMap,
    ) -> Result<String> {
        let mut stdout = self.term.create_writer(&self.config);

        self.kill_ring.reset(); // TODO recreate a new kill ring vs reset
        let mut s = State::new(&mut stdout, prompt, &self.history);

        let mut input_state = InputState::new();

        if let Some((left, right)) = initial {
            s.line.update(
                (left.to_owned() + right).as_ref(),
                left.len(),
                &mut s.changes,
            );
        }

        let mut rdr = self
            .term
            .create_reader(self.buffer.take(), &self.config, term_key_map)?;
        s.refresh_line()?;

        loop {
            let mut cmd = s.next_cmd(&mut input_state, &mut rdr, false)?;

            if cmd.should_reset_kill_ring() {
                self.kill_ring.reset();
            }

            // First trigger commands that need extra input

            if cmd == Cmd::ReverseSearchHistory {
                // Search history backward
                let next =
                    reverse_incremental_search(&mut rdr, &mut s, &mut input_state, &self.history)?;
                if let Some(next) = next {
                    cmd = next;
                } else {
                    continue;
                }
            }

            #[cfg(unix)]
            if cmd == Cmd::Suspend {
                original_mode.disable_raw_mode()?;
                tty::suspend()?;
                let _ = self.term.enable_raw_mode(&self.config)?; // TODO original_mode may have changed
                s.out.update_size(); // window may have been resized
                s.refresh_line()?;
                continue;
            }

            #[cfg(unix)]
            if cmd == Cmd::QuotedInsert {
                // Quoted insert
                let c = rdr.next_char()?;
                s.edit_insert(c, 1)?;
                continue;
            }

            #[cfg(windows)]
            if cmd == Cmd::PasteFromClipboard {
                let clipboard = rdr.read_pasted_text()?;
                s.edit_yank(&clipboard[..], 1)?;
            }

            // Tiny test quirk
            #[cfg(test)]
            if matches!(cmd, Cmd::AcceptLine) {
                self.term.cursor = s.layout.cursor.col as usize;
            }

            // Execute things can be done solely on a state object
            match command::execute(cmd, &mut s, &mut self.kill_ring)? {
                command::Status::Proceed => continue,
                command::Status::Submit => break,
            }
        }

        // Move to end, in case cursor was in the middle of the line, so that
        // next thing application prints goes after the input
        s.edit_move_buffer_end()?;

        let _ = original_mode; // silent warning
        self.buffer = rdr.unbuffer();
        Ok(s.line.into_string())
    }

    /// Add a new entry in the history.
    ///
    /// # Errors
    /// Will return `Err` if entry cannot be persisted
    pub fn add_history_entry<S: AsRef<str> + Into<String>>(&mut self, line: S) -> Result<bool> {
        self.history.add(line.as_ref())
    }

    /// Clear history.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn clear_history(&mut self) -> Result<()> {
        self.history.clear()
    }

    /// Return a mutable reference to the history object.
    pub fn history_mut(&mut self) -> &mut I {
        &mut self.history
    }

    /// Return an immutable reference to the history object.
    pub fn history(&self) -> &I {
        &self.history
    }
}

impl<H: Helper, I: History> config::Configurer for Editor<H, I> {
    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    fn set_max_history_size(&mut self, max_size: usize) -> Result<()> {
        self.config_mut().set_max_history_size(max_size);
        self.history.set_max_len(max_size)
    }

    fn set_history_ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.config_mut().set_history_ignore_dups(yes);
        self.history.ignore_dups(yes)
    }

    fn set_history_ignore_space(&mut self, yes: bool) {
        self.config_mut().set_history_ignore_space(yes);
        self.history.ignore_space(yes);
    }
}

impl<H: Helper, I: History> fmt::Debug for Editor<H, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Editor")
            .field("term", &self.term)
            .field("config", &self.config)
            .finish()
    }
}

#[cfg(test)]
mod test;
