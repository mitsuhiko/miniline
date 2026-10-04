use std::cmp::Ordering;

use crate::unicode;

/// Tell how grapheme clusters are supported / rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Default))]
pub(crate) enum GraphemeClusterMode {
    /// Support grapheme clustering
    #[cfg_attr(test, default)]
    Unicode,
    /// Doesn't support shaping
    WcWidth,
    /// Skip zero-width joiner
    NoZwj,
}

impl GraphemeClusterMode {
    /// Return default
    #[cfg(test)]
    pub fn from_env() -> Self {
        GraphemeClusterMode::default()
    }

    /// Use environment variables to guess current mode
    #[cfg(not(test))]
    pub fn from_env() -> Self {
        match std::env::var("TERM_PROGRAM").as_deref() {
            Ok("Apple_Terminal" | "iTerm.app" | "WezTerm") => GraphemeClusterMode::Unicode,
            Err(std::env::VarError::NotPresent) => match std::env::var("TERM").as_deref() {
                Ok("xterm-kitty") => GraphemeClusterMode::NoZwj,
                _ => GraphemeClusterMode::WcWidth,
            },
            _ => GraphemeClusterMode::WcWidth,
        }
    }

    /// Grapheme with / number of columns
    pub fn width(&self, s: &str) -> Unit {
        match self {
            GraphemeClusterMode::Unicode => uwidth(s),
            GraphemeClusterMode::WcWidth => wcwidth(s),
            GraphemeClusterMode::NoZwj => no_zwj(s),
        }
    }
}

/// Height, width
pub(crate) type Unit = u16;

/// Character width / number of columns
pub(crate) fn cwidh(c: char) -> Unit {
    Unit::try_from(unicode::char_width(c).unwrap_or(0)).unwrap()
}

fn uwidth(s: &str) -> Unit {
    Unit::try_from(unicode::str_width(s)).unwrap()
}

fn wcwidth(s: &str) -> Unit {
    let mut width = 0;
    for c in s.chars() {
        width += cwidh(c);
    }
    width
}

const ZWJ: char = '\u{200D}';
fn no_zwj(s: &str) -> Unit {
    let mut width = 0;
    for x in s.split(ZWJ) {
        width += uwidth(x);
    }
    width
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Position {
    pub col: Unit, // The leftmost column is number 0.
    pub row: Unit, // The highest row is number 0.
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.row.cmp(&other.row) {
            Ordering::Equal => self.col.cmp(&other.col),
            o => o,
        }
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct Layout {
    pub grapheme_cluster_mode: GraphemeClusterMode,
    /// Prompt Unicode/visible width and height
    pub prompt_size: Position,
    pub default_prompt: bool,
    /// Cursor position (relative to the start of the prompt)
    pub cursor: Position,
    /// Number of rows used so far (from start of prompt to end of input)
    pub end: Position,
}

impl Layout {
    pub fn new(grapheme_cluster_mode: GraphemeClusterMode) -> Self {
        Self {
            grapheme_cluster_mode,
            prompt_size: Position::default(),
            default_prompt: false,
            cursor: Position::default(),
            end: Position::default(),
        }
    }

    pub fn width(&self, s: &str) -> Unit {
        self.grapheme_cluster_mode.width(s)
    }
}

#[cfg(test)]
mod test {
    use super::GraphemeClusterMode;

    #[test]
    #[cfg(feature = "unicode-width")]
    fn unicode_width() {
        assert_eq!(1, super::uwidth("a"));
        assert_eq!(2, super::uwidth("👩‍🚀"));
        assert_eq!(2, super::uwidth("👋🏿"));
        assert_eq!(2, super::uwidth("👨‍👩‍👧‍👦"));
        // iTerm2, Terminal.app KO
        assert_eq!(2, super::uwidth("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
        // WezTerm KO, Terminal.app (rendered width = 1)
        assert_eq!(2, super::uwidth("❤️"));
        let gcm = GraphemeClusterMode::Unicode;
        assert_eq!(2, gcm.width("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
    }

    #[test]
    #[cfg(all(not(feature = "unicode-width"), not(feature = "unicode-segmentation")))]
    fn fallback_unicode_width() {
        assert_eq!(1, super::uwidth("a"));
        assert_eq!(2, super::uwidth("👩‍🚀"));
        assert_eq!(2, super::uwidth("👋🏿"));
        assert_eq!(2, super::uwidth("👨‍👩‍👧‍👦"));
        assert_eq!(2, super::uwidth("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
        assert_eq!(2, super::uwidth("❤️"));
        let gcm = GraphemeClusterMode::Unicode;
        assert_eq!(2, gcm.width("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
    }

    #[test]
    fn test_wcwidth() {
        assert_eq!(1, super::wcwidth("a"));
        assert_eq!(4, super::wcwidth("👩‍🚀"));
        assert_eq!(4, super::wcwidth("👋🏿"));
        assert_eq!(8, super::wcwidth("👨‍👩‍👧‍👦"));
        assert_eq!(16, super::wcwidth("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
        assert_eq!(1, super::wcwidth("❤️"));
        let gcm = GraphemeClusterMode::WcWidth;
        assert_eq!(16, gcm.width("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
    }

    #[test]
    fn test_no_zwj() {
        assert_eq!(1, super::no_zwj("a"));
        assert_eq!(4, super::no_zwj("👩‍🚀"));
        assert_eq!(2, super::no_zwj("👋🏿"));
        assert_eq!(8, super::no_zwj("👨‍👩‍👧‍👦"));
        assert_eq!(8, super::no_zwj("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
        let gcm = GraphemeClusterMode::NoZwj;
        assert_eq!(8, gcm.width("👩🏼‍👨🏼‍👦🏼‍👦🏼"));
    }

    #[test]
    #[cfg(feature = "unicode-width")]
    fn test_no_zwj_vs16() {
        assert_eq!(2, super::no_zwj("️❤️"));
    }
}
