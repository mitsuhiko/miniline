//! Tests specific definitions (also used as an "unsupported terminal"
//! fallback on platforms without terminal support)
use std::vec::IntoIter;

use super::{RawMode, RawReader, Renderer, Term};
use crate::config::Config;
use crate::error::ReadlineError;
use crate::keymap::Cmd;
use crate::keys::KeyEvent;
use crate::layout::{GraphemeClusterMode, Layout, Position, Unit};
use crate::line_buffer::LineBuffer;
use crate::Result;

pub type Buffer = ();
pub type KeyMap = ();
pub type Mode = ();

impl RawMode for Mode {
    fn disable_raw_mode(&self) -> Result<()> {
        Ok(())
    }
}

impl RawReader for IntoIter<KeyEvent> {
    type Buffer = Buffer;

    fn next_key(&mut self, _: bool) -> Result<KeyEvent> {
        match self.next() {
            Some(key) => Ok(key),
            None => Err(ReadlineError::Eof),
        }
    }

    #[cfg(unix)]
    fn next_char(&mut self) -> Result<char> {
        use crate::keys::{KeyCode as K, KeyEvent as E, Modifiers as M};
        match self.next() {
            Some(E(K::Char(c), M::NONE)) => Ok(c),
            None => Err(ReadlineError::Eof),
            _ => unimplemented!(),
        }
    }

    fn read_pasted_text(&mut self) -> Result<String> {
        Ok("pasted".to_owned())
    }

    fn find_binding(&self, _: &KeyEvent) -> Option<Cmd> {
        None
    }

    fn unbuffer(self) -> Option<Buffer> {
        None
    }
}

#[derive(Default)]
pub struct Sink {}

impl Renderer for Sink {
    type Reader = IntoIter<KeyEvent>;

    fn move_cursor(&mut self, _: Position, _: Position) -> Result<()> {
        Ok(())
    }

    fn refresh_line(
        &mut self,
        _prompt: &str,
        _line: &LineBuffer,
        _old_layout: Option<&Layout>,
        _new_layout: &Layout,
    ) -> Result<()> {
        Ok(())
    }

    fn calculate_position(&self, s: &str, orig: Position) -> Position {
        let mut pos = orig;
        pos.col += u16::try_from(s.len()).unwrap();
        pos
    }

    fn write_and_flush(&mut self, _: &str) -> Result<()> {
        Ok(())
    }

    fn clear_screen(&mut self) -> Result<()> {
        Ok(())
    }

    fn clear_to_eol(&mut self) -> Result<()> {
        Ok(())
    }

    fn update_size(&mut self) {}

    fn get_columns(&self) -> Unit {
        80
    }

    fn grapheme_cluster_mode(&self) -> GraphemeClusterMode {
        GraphemeClusterMode::Unicode
    }
}

pub type Terminal = DummyTerminal;

#[derive(Clone, Debug)]
pub struct DummyTerminal {
    pub keys: Vec<KeyEvent>,
    pub cursor: usize, // cursor position before last command
}

impl Term for DummyTerminal {
    type Buffer = Buffer;
    type KeyMap = KeyMap;
    type Mode = Mode;
    type Reader = IntoIter<KeyEvent>;
    type Writer = Sink;

    fn new(_config: &Config) -> Result<Self> {
        Ok(Self {
            keys: vec![],
            cursor: 0,
        })
    }

    // Init checks:

    #[cfg(test)]
    fn is_unsupported(&self) -> bool {
        false
    }

    #[cfg(not(test))]
    fn is_unsupported(&self) -> bool {
        true
    }

    fn is_input_tty(&self) -> bool {
        true
    }

    // Interactive loop:

    fn enable_raw_mode(&mut self, _: &Config) -> Result<(Mode, KeyMap)> {
        Ok(((), ()))
    }

    fn create_reader(&self, _: Option<Buffer>, _: &Config, _: KeyMap) -> Result<Self::Reader> {
        Ok(self.keys.clone().into_iter())
    }

    fn create_writer(&self, _: &Config) -> Sink {
        Sink::default()
    }

    fn writeln(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
pub fn suspend() -> Result<()> {
    Ok(())
}
