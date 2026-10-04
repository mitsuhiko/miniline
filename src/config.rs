//! Customize line editor
use crate::Result;

/// User preferences
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Maximum number of entries in History.
    max_history_size: usize, // history_max_entries
    history_duplicates: HistoryDuplicates,
    history_ignore_space: bool,
    /// If true, each nonblank line returned by `readline` will be
    /// automatically added to the history.
    auto_add_history: bool,
    /// Horizontal space taken by a tab.
    tab_stop: u8,
    /// Bracketed paste on unix platform
    enable_bracketed_paste: bool,
    /// Synchronized output on unix platform
    enable_synchronized_output: bool,
    /// Whether to disable or not the signals in termios
    enable_signals: bool,
}

impl Config {
    /// Returns a `Config` builder.
    #[must_use]
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Tell the maximum length (i.e. number of entries) for the history.
    #[must_use]
    pub fn max_history_size(&self) -> usize {
        self.max_history_size
    }

    pub(crate) fn set_max_history_size(&mut self, max_size: usize) {
        self.max_history_size = max_size;
    }

    /// Tell if lines which match the previous history entry are saved or not
    /// in the history list.
    ///
    /// By default, they are ignored.
    #[must_use]
    pub fn history_duplicates(&self) -> HistoryDuplicates {
        self.history_duplicates
    }

    pub(crate) fn set_history_ignore_dups(&mut self, yes: bool) {
        self.history_duplicates = if yes {
            HistoryDuplicates::IgnoreConsecutive
        } else {
            HistoryDuplicates::AlwaysAdd
        };
    }

    /// Tell if lines which begin with a space character are saved or not in
    /// the history list.
    ///
    /// By default, they are saved.
    #[must_use]
    pub fn history_ignore_space(&self) -> bool {
        self.history_ignore_space
    }

    pub(crate) fn set_history_ignore_space(&mut self, yes: bool) {
        self.history_ignore_space = yes;
    }

    /// Tell if lines are automatically added to the history.
    ///
    /// By default, they are not.
    #[must_use]
    pub fn auto_add_history(&self) -> bool {
        self.auto_add_history
    }

    /// Horizontal space taken by a tab.
    ///
    /// By default, 8.
    #[must_use]
    pub fn tab_stop(&self) -> u8 {
        self.tab_stop
    }

    pub(crate) fn set_tab_stop(&mut self, tab_stop: u8) {
        self.tab_stop = tab_stop;
    }

    /// Bracketed paste on unix platform
    ///
    /// By default, it's enabled.
    #[must_use]
    pub fn enable_bracketed_paste(&self) -> bool {
        self.enable_bracketed_paste
    }

    /// Synchronized output on unix platform
    ///
    /// By default, it's enabled.
    #[must_use]
    pub fn enable_synchronized_output(&self) -> bool {
        self.enable_synchronized_output
    }

    /// Enable or disable signals in termios
    ///
    /// By default, it's disabled.
    #[must_use]
    pub fn enable_signals(&self) -> bool {
        self.enable_signals
    }

    pub(crate) fn set_enable_signals(&mut self, enable_signals: bool) {
        self.enable_signals = enable_signals;
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_history_size: 100,
            history_duplicates: HistoryDuplicates::IgnoreConsecutive,
            history_ignore_space: false,
            auto_add_history: false,
            tab_stop: 8,
            enable_bracketed_paste: true,
            enable_synchronized_output: true,
            enable_signals: false,
        }
    }
}

/// History filter
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDuplicates {
    /// No filter
    AlwaysAdd,
    /// a line will not be added to the history if it matches the previous entry
    IgnoreConsecutive,
}

/// Configuration builder
#[derive(Clone, Debug, Default)]
pub struct Builder {
    p: Config,
}

impl Builder {
    /// Returns a [`Config`] builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            p: Config::default(),
        }
    }

    /// Set the maximum length for the history.
    pub fn max_history_size(mut self, max_size: usize) -> Result<Self> {
        self.set_max_history_size(max_size)?;
        Ok(self)
    }

    /// Tell if lines which match the previous history entry are saved or not
    /// in the history list.
    ///
    /// By default, they are ignored.
    pub fn history_ignore_dups(mut self, yes: bool) -> Result<Self> {
        self.set_history_ignore_dups(yes)?;
        Ok(self)
    }

    /// Tell if lines which begin with a space character are saved or not in
    /// the history list.
    ///
    /// By default, they are saved.
    #[must_use]
    pub fn history_ignore_space(mut self, yes: bool) -> Self {
        self.set_history_ignore_space(yes);
        self
    }

    /// Tell if lines are automatically added to the history.
    ///
    /// By default, they are not.
    #[must_use]
    pub fn auto_add_history(mut self, yes: bool) -> Self {
        self.set_auto_add_history(yes);
        self
    }

    /// Horizontal space taken by a tab.
    ///
    /// By default, `8`
    #[must_use]
    pub fn tab_stop(mut self, tab_stop: u8) -> Self {
        self.set_tab_stop(tab_stop);
        self
    }

    /// Enable or disable bracketed paste on unix platform
    ///
    /// By default, it's enabled.
    #[must_use]
    pub fn bracketed_paste(mut self, enabled: bool) -> Self {
        self.enable_bracketed_paste(enabled);
        self
    }

    /// Enable or disable signals in termios
    ///
    /// By default, it's disabled.
    #[must_use]
    pub fn enable_signals(mut self, enable_signals: bool) -> Self {
        self.set_enable_signals(enable_signals);
        self
    }

    /// Builds a [`Config`] with the settings specified so far.
    #[must_use]
    pub fn build(self) -> Config {
        self.p
    }
}

impl Configurer for Builder {
    fn config_mut(&mut self) -> &mut Config {
        &mut self.p
    }
}

/// Trait for component that holds a [`Config`].
pub trait Configurer {
    /// `Config` accessor.
    fn config_mut(&mut self) -> &mut Config;

    /// Set the maximum length for the history.
    ///
    /// # Errors
    /// Will return `Err` if size cannot be changed
    fn set_max_history_size(&mut self, max_size: usize) -> Result<()> {
        self.config_mut().set_max_history_size(max_size);
        Ok(())
    }

    /// Tell if lines which match the previous history entry are saved or not
    /// in the history list.
    ///
    /// By default, they are ignored.
    ///
    /// # Errors
    /// Will return `Err` if this setting cannot be changed
    fn set_history_ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.config_mut().set_history_ignore_dups(yes);
        Ok(())
    }

    /// Tell if lines which begin with a space character are saved or not in
    /// the history list.
    ///
    /// By default, they are saved.
    fn set_history_ignore_space(&mut self, yes: bool) {
        self.config_mut().set_history_ignore_space(yes);
    }

    /// Tell if lines are automatically added to the history.
    ///
    /// By default, they are not.
    fn set_auto_add_history(&mut self, yes: bool) {
        self.config_mut().auto_add_history = yes;
    }

    /// Horizontal space taken by a tab.
    ///
    /// By default, `8`
    fn set_tab_stop(&mut self, tab_stop: u8) {
        self.config_mut().set_tab_stop(tab_stop);
    }

    /// Enable or disable bracketed paste on unix platform
    ///
    /// By default, it's enabled.
    fn enable_bracketed_paste(&mut self, enabled: bool) {
        self.config_mut().enable_bracketed_paste = enabled;
    }

    /// Enable or disable synchronized output on unix platform
    ///
    /// By default, it's enabled.
    fn enable_synchronized_output(&mut self, enabled: bool) {
        self.config_mut().enable_synchronized_output = enabled;
    }

    /// Enable or disable signals in termios
    ///
    /// By default, it's disabled.
    fn set_enable_signals(&mut self, enable_signals: bool) {
        self.config_mut().set_enable_signals(enable_signals);
    }
}

impl Configurer for Config {
    fn config_mut(&mut self) -> &mut Config {
        self
    }
}
