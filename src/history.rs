//! History API

use std::borrow::Cow;
use std::collections::{vec_deque, VecDeque};
use std::ops::Index;

use super::Result;
use crate::config::{Config, HistoryDuplicates};

/// Search direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirection {
    /// Search history forward
    Forward,
    /// Search history backward
    Reverse,
}

/// History search result
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SearchResult<'a> {
    /// history entry
    pub entry: Cow<'a, str>,
    /// history index
    pub idx: usize,
    /// match position in `entry`
    pub pos: usize,
}

/// Interface for navigating history
pub trait History {
    /// Return the history entry at position `index`, starting from 0.
    ///
    /// `SearchDirection` is useful only for implementations without direct
    /// indexing.
    ///
    /// # Errors
    /// Will return `Err` if history entry cannot be retrieved
    fn get(&self, index: usize, dir: SearchDirection) -> Result<Option<SearchResult<'_>>>;

    /// Add a new entry in the history.
    ///
    /// Return false if the `line` has been ignored (blank line / duplicate /
    /// ...).
    ///
    /// # Errors
    /// Will return `Err` if entry cannot be persisted
    fn add(&mut self, line: &str) -> Result<bool>;

    /// Add a new entry in the history.
    ///
    /// Return false if the `line` has been ignored (blank line / duplicate /
    /// ...).
    ///
    /// # Errors
    /// Will return `Err` if entry cannot be persisted
    fn add_owned(&mut self, line: String) -> Result<bool>;

    /// Return the number of entries in the history.
    #[must_use]
    fn len(&self) -> usize;

    /// Return true if the history has no entry.
    #[must_use]
    fn is_empty(&self) -> bool;

    /// Set the maximum length for the history. This function can be called even
    /// if there is already some history, the function will make sure to retain
    /// just the latest `len` elements if the new history length value is
    /// smaller than the amount of items already inside the history.
    ///
    /// Like [stifle_history](http://tiswww.case.edu/php/chet/readline/history.html#IDX11).
    ///
    /// # Errors
    /// Will return `Err` if the size cannot be changed
    fn set_max_len(&mut self, len: usize) -> Result<()>;

    /// Ignore consecutive duplicates
    ///
    /// # Errors
    /// Will return `Err` if this setting cannot be changed
    fn ignore_dups(&mut self, yes: bool) -> Result<()>;

    /// Ignore lines which begin with a space or not
    fn ignore_space(&mut self, yes: bool);

    /// Clear in-memory history
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    fn clear(&mut self) -> Result<()>;

    /// Search history (start position inclusive [0, len-1]).
    ///
    /// Return the absolute index of the nearest history entry that matches
    /// `term`.
    ///
    /// Return None if no entry contains `term` between [start, len -1] for
    /// forward search
    /// or between [0, start] for reverse search.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    fn search(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>>;

    /// Anchored search
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>>;
}

/// Transient in-memory history implementation.
pub struct MemHistory {
    entries: VecDeque<String>,
    max_len: usize,
    ignore_space: bool,
    ignore_dups: bool,
}

impl MemHistory {
    /// Default constructor
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(&Config::default())
    }

    /// Customized constructor with:
    /// - [`Config::max_history_size()`],
    /// - [`Config::history_ignore_space()`],
    /// - [`Config::history_duplicates()`].
    #[must_use]
    pub fn with_config(config: &Config) -> Self {
        Self {
            entries: VecDeque::new(),
            max_len: config.max_history_size(),
            ignore_space: config.history_ignore_space(),
            ignore_dups: config.history_duplicates() == HistoryDuplicates::IgnoreConsecutive,
        }
    }

    fn search_match<F>(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
        test: F,
    ) -> Option<SearchResult<'_>>
    where
        F: Fn(&str) -> Option<usize>,
    {
        if term.is_empty() || start >= self.len() {
            return None;
        }
        match dir {
            SearchDirection::Reverse => {
                for (idx, entry) in self
                    .entries
                    .iter()
                    .rev()
                    .skip(self.len() - 1 - start)
                    .enumerate()
                {
                    if let Some(cursor) = test(entry) {
                        return Some(SearchResult {
                            idx: start - idx,
                            entry: Cow::Borrowed(entry),
                            pos: cursor,
                        });
                    }
                }
                None
            }
            SearchDirection::Forward => {
                for (idx, entry) in self.entries.iter().skip(start).enumerate() {
                    if let Some(cursor) = test(entry) {
                        return Some(SearchResult {
                            idx: idx + start,
                            entry: Cow::Borrowed(entry),
                            pos: cursor,
                        });
                    }
                }
                None
            }
        }
    }

    fn ignore(&self, line: &str) -> bool {
        if self.max_len == 0 {
            return true;
        }
        if line.is_empty()
            || (self.ignore_space && line.chars().next().map_or(true, char::is_whitespace))
        {
            return true;
        }
        if self.ignore_dups {
            if let Some(s) = self.entries.back() {
                if s == line {
                    return true;
                }
            }
        }
        false
    }

    fn insert(&mut self, line: String) {
        if self.entries.len() == self.max_len {
            self.entries.pop_front();
        }
        self.entries.push_back(line);
    }
}

impl Default for MemHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl History for MemHistory {
    fn get(&self, index: usize, _: SearchDirection) -> Result<Option<SearchResult<'_>>> {
        Ok(self
            .entries
            .get(index)
            .map(String::as_ref)
            .map(Cow::Borrowed)
            .map(|entry| SearchResult {
                entry,
                idx: index,
                pos: 0,
            }))
    }

    fn add(&mut self, line: &str) -> Result<bool> {
        if self.ignore(line) {
            return Ok(false);
        }
        self.insert(line.to_owned());
        Ok(true)
    }

    fn add_owned(&mut self, line: String) -> Result<bool> {
        if self.ignore(&line) {
            return Ok(false);
        }
        self.insert(line);
        Ok(true)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn set_max_len(&mut self, len: usize) -> Result<()> {
        self.max_len = len;
        if self.len() > len {
            self.entries.drain(..self.len() - len);
        }
        Ok(())
    }

    fn ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.ignore_dups = yes;
        Ok(())
    }

    fn ignore_space(&mut self, yes: bool) {
        self.ignore_space = yes;
    }

    fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        Ok(())
    }

    fn search(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        let test = |entry: &str| entry.find(term);
        Ok(self.search_match(term, start, dir, test))
    }

    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        let test = |entry: &str| {
            if entry.starts_with(term) {
                Some(term.len())
            } else {
                None
            }
        };
        Ok(self.search_match(term, start, dir, test))
    }
}

impl Index<usize> for MemHistory {
    type Output = String;

    fn index(&self, index: usize) -> &String {
        &self.entries[index]
    }
}

impl<'a> IntoIterator for &'a MemHistory {
    type IntoIter = vec_deque::Iter<'a, String>;
    type Item = &'a String;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// Default transient in-memory history implementation
pub type DefaultHistory = MemHistory;

#[cfg(test)]
mod tests {
    use super::{DefaultHistory, History as _, SearchDirection, SearchResult};
    use crate::config::Config;
    use crate::Result;

    fn init() -> DefaultHistory {
        let mut history = DefaultHistory::new();
        assert!(history.add("line1").unwrap());
        assert!(history.add("line2").unwrap());
        assert!(history.add("line3").unwrap());
        history
    }

    #[test]
    fn new() {
        let history = DefaultHistory::new();
        assert_eq!(0, history.len());
    }

    #[test]
    fn add() {
        let config = Config::builder().history_ignore_space(true).build();
        let mut history = DefaultHistory::with_config(&config);
        assert_eq!(config.max_history_size(), history.max_len);
        assert!(history.add("line1").unwrap());
        assert!(history.add("line2").unwrap());
        assert!(!history.add("line2").unwrap());
        assert!(!history.add("").unwrap());
        assert!(!history.add(" line3").unwrap());
    }

    #[test]
    fn set_max_len() {
        let mut history = init();
        history.set_max_len(1).unwrap();
        assert_eq!(1, history.len());
        assert_eq!(Some(&"line3".to_owned()), history.into_iter().last());
    }

    #[test]
    fn clear() -> Result<()> {
        let mut history = init();
        assert_eq!(3, history.len());
        history.clear()?;
        assert_eq!(0, history.len());
        Ok(())
    }

    #[test]
    fn ignore_dups() -> Result<()> {
        let mut history = init();
        assert_eq!(3, history.len());
        history.ignore_dups(false)?;
        history.add("line3")?;
        assert_eq!(4, history.len());
        Ok(())
    }

    #[test]
    fn ignore_space() -> Result<()> {
        let mut history = init();
        assert_eq!(3, history.len());
        history.ignore_space(true);
        history.add(" line4")?;
        assert_eq!(3, history.len());
        Ok(())
    }

    #[test]
    fn search() -> Result<()> {
        let history = init();
        assert_eq!(None, history.search("", 0, SearchDirection::Forward)?);
        assert_eq!(None, history.search("none", 0, SearchDirection::Forward)?);
        assert_eq!(None, history.search("line", 3, SearchDirection::Forward)?);

        assert_eq!(
            Some(SearchResult {
                idx: 0,
                entry: history.get(0, SearchDirection::Forward)?.unwrap().entry,
                pos: 0
            }),
            history.search("line", 0, SearchDirection::Forward)?
        );
        assert_eq!(
            Some(SearchResult {
                idx: 1,
                entry: history.get(1, SearchDirection::Forward)?.unwrap().entry,
                pos: 0
            }),
            history.search("line", 1, SearchDirection::Forward)?
        );
        assert_eq!(
            Some(SearchResult {
                idx: 2,
                entry: history.get(2, SearchDirection::Forward)?.unwrap().entry,
                pos: 0
            }),
            history.search("line3", 1, SearchDirection::Forward)?
        );
        Ok(())
    }

    #[test]
    fn reverse_search() -> Result<()> {
        let history = init();
        assert_eq!(None, history.search("", 2, SearchDirection::Reverse)?);
        assert_eq!(None, history.search("none", 2, SearchDirection::Reverse)?);
        assert_eq!(None, history.search("line", 3, SearchDirection::Reverse)?);

        assert_eq!(
            Some(SearchResult {
                idx: 2,
                entry: history.get(2, SearchDirection::Reverse)?.unwrap().entry,
                pos: 0
            }),
            history.search("line", 2, SearchDirection::Reverse)?
        );
        assert_eq!(
            Some(SearchResult {
                idx: 1,
                entry: history.get(1, SearchDirection::Reverse)?.unwrap().entry,
                pos: 0
            }),
            history.search("line", 1, SearchDirection::Reverse)?
        );
        assert_eq!(
            Some(SearchResult {
                idx: 0,
                entry: history.get(0, SearchDirection::Reverse)?.unwrap().entry,
                pos: 0
            }),
            history.search("line1", 1, SearchDirection::Reverse)?
        );
        Ok(())
    }

    #[test]
    fn anchored_search() -> Result<()> {
        let history = init();
        assert_eq!(
            Some(SearchResult {
                idx: 2,
                entry: history.get(2, SearchDirection::Reverse)?.unwrap().entry,
                pos: 4
            }),
            history.starts_with("line", 2, SearchDirection::Reverse)?
        );
        assert_eq!(
            None,
            history.starts_with("ine", 2, SearchDirection::Reverse)?
        );
        Ok(())
    }
}
