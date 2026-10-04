//! Bindings from keys to command (Emacs mode)
use crate::keys::{KeyCode as K, KeyEvent, KeyEvent as E, Modifiers as M};
use crate::tty::RawReader;
use crate::Result;

/// The number of times one command should be repeated.
pub(crate) type RepeatCount = u16;

/// Commands
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Cmd {
    /// abort
    Abort, // Miscellaneous Command
    /// accept-line
    AcceptLine,
    /// beginning-of-history
    BeginningOfHistory,
    /// capitalize-word
    CapitalizeWord,
    /// clear-screen
    ClearScreen,
    /// Paste from the clipboard
    #[cfg(windows)]
    PasteFromClipboard,
    /// downcase-word
    DowncaseWord,
    /// vi-eof-maybe
    EndOfFile,
    /// end-of-history
    EndOfHistory,
    /// forward-search-history (incremental search)
    ForwardSearchHistory,
    /// Insert text
    Insert(RepeatCount, String),
    /// Interrupt signal (Ctrl-C)
    Interrupt,
    /// backward-delete-char, backward-kill-line, backward-kill-word
    /// delete-char, kill-line, kill-word, unix-line-discard, unix-word-rubout
    Kill(Movement),
    /// backward-char, backward-word, beginning-of-line, end-of-line,
    /// forward-char, forward-word, character-search
    Move(Movement),
    /// next-history
    NextHistory,
    /// No action
    Noop,
    /// previous-history
    PreviousHistory,
    /// quoted-insert
    QuotedInsert,
    /// reverse-search-history (incremental search)
    ReverseSearchHistory,
    /// self-insert
    SelfInsert(RepeatCount, char),
    /// Suspend signal (Ctrl-Z on unix platform)
    #[cfg(unix)]
    Suspend,
    /// transpose-chars
    TransposeChars,
    /// transpose-words
    TransposeWords(RepeatCount),
    /// undo
    Undo(RepeatCount),
    /// Unsupported / unexpected
    Unknown,
    /// upcase-word
    UpcaseWord,
    /// yank
    Yank(RepeatCount),
    /// yank-pop
    YankPop,
    /// moves cursor to the line above or switches to prev history entry if
    /// the cursor is already on the first line
    LineUpOrPreviousHistory(RepeatCount),
    /// moves cursor to the line below or switches to next history entry if
    /// the cursor is already on the last line
    LineDownOrNextHistory(RepeatCount),
}

impl Cmd {
    /// Tells if current command should reset kill ring.
    pub const fn should_reset_kill_ring(&self) -> bool {
        match *self {
            Self::Kill(Movement::BackwardChar(_) | Movement::ForwardChar(_)) => true,
            Self::ClearScreen | Self::Kill(_) | Self::Noop | Self::Yank(..) | Self::YankPop => {
                false
            }
            #[cfg(unix)]
            Self::Suspend => false,
            _ => true,
        }
    }
}

/// Different word definitions
#[derive(Debug, Clone, Eq, PartialEq, Copy)]
pub(crate) enum Word {
    /// non-blanks characters
    Big,
    /// alphanumeric characters
    Emacs,
}

/// character search
#[derive(Debug, Clone, Eq, PartialEq, Copy)]
pub(crate) enum CharSearch {
    /// Forward search until
    ForwardBefore(char),
    /// Backward search
    Backward(char),
}

/// Where to move
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Movement {
    /// beginning-of-line
    BeginningOfLine,
    /// end-of-line
    EndOfLine,
    /// backward-word
    BackwardWord(RepeatCount, Word), // Backward until start of word
    /// forward-word
    ForwardWord(RepeatCount, Word), // Forward until after end of word
    /// character-search, character-search-backward
    ViCharSearch(RepeatCount, CharSearch),
    /// backward-char
    BackwardChar(RepeatCount),
    /// forward-char
    ForwardChar(RepeatCount),
}

/// Transform key(s) to commands based on current input mode
pub(crate) struct InputState {
    // numeric arguments: http://web.mit.edu/gnu/doc/html/rlman_1.html#SEC7
    num_args: i16,
}

pub(crate) trait Refresher {
    /// Rewrite the currently edited line accordingly to the buffer content,
    /// cursor position, and number of columns of the terminal.
    fn refresh_line(&mut self) -> Result<()>;
    /// Same as `refresh_line` but with a dynamic prompt.
    fn refresh_prompt_and_line(&mut self, prompt: &str) -> Result<()>;
    /// currently edited line
    fn line(&self) -> &str;
}

impl InputState {
    pub fn new() -> Self {
        Self { num_args: 0 }
    }

    /// Parse user input into one command
    /// `single_esc_abort` is used in emacs mode on unix platform when a single
    /// esc key is expected to abort current action.
    pub fn next_cmd<R: RawReader>(
        &mut self,
        rdr: &mut R,
        wrt: &mut dyn Refresher,
        single_esc_abort: bool,
    ) -> Result<Cmd> {
        let key = rdr.next_key(single_esc_abort)?;
        self.emacs(rdr, wrt, key)
    }

    /// Terminal peculiar binding
    fn term_binding<R: RawReader>(rdr: &R, wrt: &dyn Refresher, key: &KeyEvent) -> Option<Cmd> {
        let cmd = rdr.find_binding(key);
        if cmd == Some(Cmd::EndOfFile) && !wrt.line().is_empty() {
            None // ReadlineError::Eof only if line is empty
        } else {
            cmd
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn emacs_digit_argument<R: RawReader>(
        &mut self,
        rdr: &mut R,
        wrt: &mut dyn Refresher,
        digit: char,
    ) -> Result<KeyEvent> {
        match digit {
            '0'..='9' => {
                self.num_args = digit.to_digit(10).unwrap() as i16;
            }
            '-' => {
                self.num_args = -1;
            }
            _ => unreachable!(),
        }
        loop {
            wrt.refresh_prompt_and_line(&format!("(arg: {}) ", self.num_args))?;
            let key = rdr.next_key(true)?;
            match key {
                E(K::Char(digit @ '0'..='9'), m) if m == M::NONE || m == M::ALT => {
                    if self.num_args == -1 {
                        self.num_args *= digit.to_digit(10).unwrap() as i16;
                    } else if self.num_args.abs() < 1000 {
                        // shouldn't ever need more than 4 digits
                        self.num_args = self
                            .num_args
                            .saturating_mul(10)
                            .saturating_add(digit.to_digit(10).unwrap() as i16);
                    }
                }
                E(K::Char('-'), m) if m == M::NONE || m == M::ALT => {}
                _ => {
                    wrt.refresh_line()?;
                    return Ok(key);
                }
            };
        }
    }

    fn emacs<R: RawReader>(
        &mut self,
        rdr: &mut R,
        wrt: &mut dyn Refresher,
        mut key: KeyEvent,
    ) -> Result<Cmd> {
        if let E(K::Char(digit @ '-'), M::ALT) = key {
            key = self.emacs_digit_argument(rdr, wrt, digit)?;
        } else if let E(K::Char(digit @ '0'..='9'), M::ALT) = key {
            key = self.emacs_digit_argument(rdr, wrt, digit)?;
        }
        let (n, positive) = self.emacs_num_args(); // consume them in all cases

        if let Some(cmd) = InputState::term_binding(rdr, wrt, &key) {
            return Ok(cmd);
        }
        let cmd = match key {
            E(K::Char(c), M::NONE) => {
                if positive {
                    Cmd::SelfInsert(n, c)
                } else {
                    Cmd::Unknown
                }
            }
            E(K::Char('A'), M::CTRL) => Cmd::Move(Movement::BeginningOfLine),
            E(K::Char('B'), M::CTRL) => Cmd::Move(if positive {
                Movement::BackwardChar(n)
            } else {
                Movement::ForwardChar(n)
            }),
            E(K::Char('E'), M::CTRL) => Cmd::Move(Movement::EndOfLine),
            E(K::Char('F'), M::CTRL) => Cmd::Move(if positive {
                Movement::ForwardChar(n)
            } else {
                Movement::BackwardChar(n)
            }),
            E(K::Char('G'), M::CTRL | M::CTRL_ALT) | E::ESC => Cmd::Abort,
            E(K::Char('H'), M::CTRL) | E::BACKSPACE => Cmd::Kill(if positive {
                Movement::BackwardChar(n)
            } else {
                Movement::ForwardChar(n)
            }),
            // Without a completer `Tab` / `BackTab` (complete /
            // complete-backward) have no effect.
            E(K::BackTab, M::NONE) | E(K::Char('I'), M::CTRL) | E(K::Tab, M::NONE) => Cmd::Unknown,
            E(K::Char('K'), M::CTRL) => Cmd::Kill(if positive {
                Movement::EndOfLine
            } else {
                Movement::BeginningOfLine
            }),
            E(K::Char('L'), M::CTRL) => Cmd::ClearScreen,
            E(K::Char('N'), M::CTRL) => Cmd::NextHistory,
            E(K::Char('P'), M::CTRL) => Cmd::PreviousHistory,
            E(K::Char('X'), M::CTRL) => {
                let snd_key = rdr.next_key(true)?;
                match snd_key {
                    E(K::Char('G'), M::CTRL) | E::ESC => Cmd::Abort,
                    E(K::Char('U'), M::CTRL) => Cmd::Undo(n),
                    E(K::Backspace, M::NONE) => Cmd::Kill(if positive {
                        Movement::BeginningOfLine
                    } else {
                        Movement::EndOfLine
                    }),
                    _ => Cmd::Unknown,
                }
            }
            // character-search, character-search-backward
            E(K::Char(']'), m @ (M::CTRL | M::CTRL_ALT)) => {
                let ch = rdr.next_key(false)?;
                match ch {
                    E(K::Char(ch), M::NONE) => Cmd::Move(Movement::ViCharSearch(
                        n,
                        if positive {
                            if m.contains(M::ALT) {
                                CharSearch::Backward(ch)
                            } else {
                                CharSearch::ForwardBefore(ch)
                            }
                        } else if m.contains(M::ALT) {
                            CharSearch::ForwardBefore(ch)
                        } else {
                            CharSearch::Backward(ch)
                        },
                    )),
                    _ => Cmd::Unknown,
                }
            }
            E(K::Backspace, M::ALT) => Cmd::Kill(if positive {
                Movement::BackwardWord(n, Word::Emacs)
            } else {
                Movement::ForwardWord(n, Word::Emacs)
            }),
            E(K::Char('<'), M::ALT) => Cmd::BeginningOfHistory,
            E(K::Char('>'), M::ALT) => Cmd::EndOfHistory,
            E(K::Char('B' | 'b') | K::Left, M::ALT) | E(K::Left, M::CTRL) => {
                Cmd::Move(if positive {
                    Movement::BackwardWord(n, Word::Emacs)
                } else {
                    Movement::ForwardWord(n, Word::Emacs)
                })
            }
            E(K::Char('C' | 'c'), M::ALT) => Cmd::CapitalizeWord,
            E(K::Char('D' | 'd'), M::ALT) => Cmd::Kill(if positive {
                Movement::ForwardWord(n, Word::Emacs)
            } else {
                Movement::BackwardWord(n, Word::Emacs)
            }),
            E(K::Char('F' | 'f') | K::Right, M::ALT) | E(K::Right, M::CTRL) => {
                Cmd::Move(if positive {
                    Movement::ForwardWord(n, Word::Emacs)
                } else {
                    Movement::BackwardWord(n, Word::Emacs)
                })
            }
            E(K::Char('L' | 'l'), M::ALT) => Cmd::DowncaseWord,
            E(K::Char('T' | 't'), M::ALT) => Cmd::TransposeWords(n),
            // TODO ESC-R (r): Undo all changes made to this line.
            E(K::Char('U' | 'u'), M::ALT) => Cmd::UpcaseWord,
            E(K::Char('Y' | 'y'), M::ALT) => Cmd::YankPop,
            _ => self.common(rdr, wrt, key, n, positive)?,
        };
        Ok(cmd)
    }

    fn common<R: RawReader>(
        &mut self,
        rdr: &mut R,
        wrt: &dyn Refresher,
        key: KeyEvent,
        n: RepeatCount,
        positive: bool,
    ) -> Result<Cmd> {
        Ok(match key {
            E(K::Home, M::NONE) => Cmd::Move(Movement::BeginningOfLine),
            E(K::Left, M::NONE) => Cmd::Move(if positive {
                Movement::BackwardChar(n)
            } else {
                Movement::ForwardChar(n)
            }),
            #[cfg(any(windows, test))]
            E(K::Char('C'), M::CTRL) => Cmd::Interrupt,
            E(K::Char('D'), M::CTRL) => {
                if !wrt.line().is_empty() {
                    Cmd::Kill(if positive {
                        Movement::ForwardChar(n)
                    } else {
                        Movement::BackwardChar(n)
                    })
                } else if cfg!(windows) || cfg!(test) {
                    Cmd::EndOfFile
                } else {
                    Cmd::Unknown
                }
            }
            E(K::Delete, M::NONE) => Cmd::Kill(if positive {
                Movement::ForwardChar(n)
            } else {
                Movement::BackwardChar(n)
            }),
            E(K::End, M::NONE) => Cmd::Move(Movement::EndOfLine),
            E(K::Right, M::NONE) => Cmd::Move(if positive {
                Movement::ForwardChar(n)
            } else {
                Movement::BackwardChar(n)
            }),
            E(K::Char('J' | 'M'), M::CTRL) | E::ENTER => Cmd::AcceptLine,
            E(K::Down, M::NONE) => Cmd::LineDownOrNextHistory(1),
            E(K::Up, M::NONE) => Cmd::LineUpOrPreviousHistory(1),
            E(K::Char('R'), M::CTRL) => Cmd::ReverseSearchHistory,
            // most terminals override Ctrl+S to suspend execution
            E(K::Char('S'), M::CTRL) => Cmd::ForwardSearchHistory,
            E(K::Char('T'), M::CTRL) => Cmd::TransposeChars,
            E(K::Char('U'), M::CTRL) => Cmd::Kill(if positive {
                Movement::BeginningOfLine
            } else {
                Movement::EndOfLine
            }),
            // most terminals override Ctrl+Q to resume execution
            E(K::Char('Q'), M::CTRL) => Cmd::QuotedInsert,
            #[cfg(not(windows))]
            E(K::Char('V'), M::CTRL) => Cmd::QuotedInsert,
            #[cfg(windows)]
            E(K::Char('V'), M::CTRL) => Cmd::PasteFromClipboard,
            E(K::Char('W'), M::CTRL) => Cmd::Kill(if positive {
                Movement::BackwardWord(n, Word::Big)
            } else {
                Movement::ForwardWord(n, Word::Big)
            }),
            E(K::Char('Y'), M::CTRL) => {
                if positive {
                    Cmd::Yank(n)
                } else {
                    Cmd::Unknown // TODO Validate
                }
            }
            E(K::Char('_'), M::CTRL) => Cmd::Undo(n),
            E(K::UnknownEscSeq, M::NONE) => Cmd::Noop,
            E(K::BracketedPasteStart, M::NONE) => {
                let paste = rdr.read_pasted_text()?;
                Cmd::Insert(1, paste)
            }
            _ => Cmd::Unknown,
        })
    }

    fn num_args(&mut self) -> i16 {
        let num_args = match self.num_args {
            0 => 1,
            _ => self.num_args,
        };
        self.num_args = 0;
        num_args
    }

    #[allow(clippy::cast_sign_loss)]
    fn emacs_num_args(&mut self) -> (RepeatCount, bool) {
        let num_args = self.num_args();
        if num_args < 0 {
            if let (n, false) = num_args.overflowing_abs() {
                (n as RepeatCount, false)
            } else {
                (RepeatCount::MAX, false)
            }
        } else {
            (num_args as RepeatCount, true)
        }
    }
}
