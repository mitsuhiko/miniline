//! Key constants
use std::ops::{BitOr, BitOrAssign};

/// Input key pressed and modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct KeyEvent(pub KeyCode, pub Modifiers);

impl KeyEvent {
    /// Constant value representing an unmodified press of `KeyCode::Backspace`.
    pub(crate) const BACKSPACE: Self = Self(KeyCode::Backspace, Modifiers::NONE);
    /// Constant value representing an unmodified press of `KeyCode::Enter`.
    pub(crate) const ENTER: Self = Self(KeyCode::Enter, Modifiers::NONE);
    /// Constant value representing an unmodified press of `KeyCode::Esc`.
    pub(crate) const ESC: Self = Self(KeyCode::Esc, Modifiers::NONE);

    /// Constructor from `char` and modifiers
    pub fn new(c: char, mut mods: Modifiers) -> Self {
        use self::{KeyCode as K, KeyEvent as E, Modifiers as M};

        if !c.is_control() {
            if !mods.is_empty() {
                mods.remove(M::SHIFT); // TODO Validate: no SHIFT even if
                                       // `c` is uppercase
            }
            return E(K::Char(c), mods);
        }
        match c {
            '\x00' => E(K::Char('@'), mods | M::CTRL), // '\0'
            '\x01' => E(K::Char('A'), mods | M::CTRL),
            '\x02' => E(K::Char('B'), mods | M::CTRL),
            '\x03' => E(K::Char('C'), mods | M::CTRL),
            '\x04' => E(K::Char('D'), mods | M::CTRL),
            '\x05' => E(K::Char('E'), mods | M::CTRL),
            '\x06' => E(K::Char('F'), mods | M::CTRL),
            '\x07' => E(K::Char('G'), mods | M::CTRL), // '\a'
            #[cfg(not(windows))]
            '\x08' => E(K::Backspace, mods), // '\b'
            #[cfg(windows)]
            '\x08' => E(K::Char('H'), mods | M::CTRL),
            #[cfg(not(windows))]
            '\x09' => {
                // '\t'
                if mods.contains(M::SHIFT) {
                    mods.remove(M::SHIFT);
                    E(K::BackTab, mods)
                } else {
                    E(K::Tab, mods)
                }
            }
            #[cfg(windows)]
            '\x09' => E(K::Char('I'), mods | M::CTRL),
            '\x0a' => E(K::Char('J'), mods | M::CTRL), // '\n' (10)
            '\x0b' => E(K::Char('K'), mods | M::CTRL),
            '\x0c' => E(K::Char('L'), mods | M::CTRL),
            #[cfg(not(windows))]
            '\x0d' => E(K::Enter, mods), // '\r' (13)
            #[cfg(windows)]
            '\x0d' => E(K::Char('M'), mods | M::CTRL),
            '\x0e' => E(K::Char('N'), mods | M::CTRL),
            '\x0f' => E(K::Char('O'), mods | M::CTRL),
            '\x10' => E(K::Char('P'), mods | M::CTRL),
            '\x11' => E(K::Char('Q'), mods | M::CTRL),
            '\x12' => E(K::Char('R'), mods | M::CTRL),
            '\x13' => E(K::Char('S'), mods | M::CTRL),
            '\x14' => E(K::Char('T'), mods | M::CTRL),
            '\x15' => E(K::Char('U'), mods | M::CTRL),
            '\x16' => E(K::Char('V'), mods | M::CTRL),
            '\x17' => E(K::Char('W'), mods | M::CTRL),
            '\x18' => E(K::Char('X'), mods | M::CTRL),
            '\x19' => E(K::Char('Y'), mods | M::CTRL),
            '\x1a' => E(K::Char('Z'), mods | M::CTRL),
            '\x1b' => E(K::Esc, mods), // Ctrl-[, '\e'
            '\x1c' => E(K::Char('\\'), mods | M::CTRL),
            '\x1d' => E(K::Char(']'), mods | M::CTRL),
            '\x1e' => E(K::Char('^'), mods | M::CTRL),
            '\x1f' => E(K::Char('_'), mods | M::CTRL),
            '\x7f' => E(K::Backspace, mods), // Rubout, Ctrl-?
            '\u{9b}' => E(K::Esc, mods | M::SHIFT),
            _ => E(K::Null, mods),
        }
    }

    /// Constructor from `char` with Ctrl modifier
    #[cfg(test)]
    pub fn ctrl(c: char) -> Self {
        Self::new(c, Modifiers::CTRL)
    }

    /// Constructor from `char` with Alt modifier
    #[cfg(any(unix, test))]
    pub fn alt(c: char) -> Self {
        Self::new(c, Modifiers::ALT)
    }
}

impl From<char> for KeyEvent {
    fn from(c: char) -> Self {
        Self::new(c, Modifiers::NONE)
    }
}

/// Input key pressed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // not all keys are produced on all platforms
pub(crate) enum KeyCode {
    /// Unsupported escape sequence (on unix platform)
    UnknownEscSeq,
    /// ⌫ or Ctrl-H
    Backspace,
    /// ⇤ (usually Shift-Tab)
    BackTab,
    /// Paste (on unix platform)
    BracketedPasteStart,
    /// Paste (on unix platform)
    BracketedPasteEnd,
    /// Single char
    Char(char),
    /// ⌦
    Delete,
    /// ↓ arrow key
    Down,
    /// ⇲
    End,
    /// ↵ or Ctrl-M
    Enter,
    /// Escape or Ctrl-[
    Esc,
    /// Function key
    F(u8),
    /// ⇱
    Home,
    /// Insert key
    Insert,
    /// ← arrow key
    Left,
    /// \0
    Null,
    /// ⇟
    PageDown,
    /// ⇞
    PageUp,
    /// → arrow key
    Right,
    /// ⇥ or Ctrl-I
    Tab,
    /// ↑ arrow key
    Up,
}

/// The set of modifier keys that were triggered along with a key press.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Modifiers(u8);

#[allow(dead_code)] // not all combinations are produced on all platforms
impl Modifiers {
    /// Control modifier
    pub const CTRL: Self = Self(1 << 3);
    /// Escape or Alt modifier
    pub const ALT: Self = Self(1 << 2);
    /// Shift modifier
    pub const SHIFT: Self = Self(1 << 1);

    /// No modifier
    pub const NONE: Self = Self(0);
    /// Ctrl + Shift
    pub const CTRL_SHIFT: Self = Self(Self::CTRL.0 | Self::SHIFT.0);
    /// Alt + Shift
    pub const ALT_SHIFT: Self = Self(Self::ALT.0 | Self::SHIFT.0);
    /// Ctrl + Alt
    pub const CTRL_ALT: Self = Self(Self::CTRL.0 | Self::ALT.0);
    /// Ctrl + Alt + Shift
    pub const CTRL_ALT_SHIFT: Self = Self(Self::CTRL.0 | Self::ALT.0 | Self::SHIFT.0);

    /// Returns `true` if no modifier is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if all modifiers in `other` are set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Removes the modifiers in `other`.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::KeyCode as K;
    use super::{KeyEvent as E, Modifiers as M};

    #[test]
    fn new() {
        assert_eq!(E::ESC, E::new('\x1b', M::NONE));
    }

    #[test]
    #[cfg(unix)]
    fn from() {
        assert_eq!(E(K::Tab, M::NONE), E::from('\t'));
    }

    #[test]
    #[cfg(windows)]
    fn from() {
        assert_eq!(E(K::Char('I'), M::CTRL), E::from('\t'));
    }
}
