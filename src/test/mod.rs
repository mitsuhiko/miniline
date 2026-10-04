use crate::config::Config;
use crate::history::History as _;
use crate::keys::{KeyCode as K, KeyEvent, KeyEvent as E, Modifiers as M};
use crate::{apply_backspace_direct, readline_direct, DefaultEditor};

mod common;
mod emacs;
mod history;

fn init_editor(keys: &[KeyEvent]) -> DefaultEditor {
    let config = Config::builder().build();
    let mut editor = DefaultEditor::with_config(config).unwrap();
    editor.term.keys.extend(keys.iter().copied());
    editor
}

// `keys`: keys to press
// `expected_line`: line after enter key
fn assert_line(keys: &[KeyEvent], expected_line: &str) {
    let mut editor = init_editor(keys);
    let actual_line = editor.readline(">>").unwrap();
    assert_eq!(expected_line, actual_line);
}

// `initial`: line status before `keys` pressed: strings before and after cursor
// `keys`: keys to press
// `expected_line`: line after enter key
fn assert_line_with_initial(initial: (&str, &str), keys: &[KeyEvent], expected_line: &str) {
    let mut editor = init_editor(keys);
    let actual_line = editor.readline_with_initial(">>", initial).unwrap();
    assert_eq!(expected_line, actual_line);
}

// `initial`: line status before `keys` pressed: strings before and after cursor
// `keys`: keys to press
// `expected`: line status before enter key: strings before and after cursor
fn assert_cursor(initial: (&str, &str), keys: &[KeyEvent], expected: (&str, &str)) {
    let mut editor = init_editor(keys);
    let actual_line = editor.readline_with_initial("", initial).unwrap();
    assert_eq!(expected.0.to_owned() + expected.1, actual_line);
    assert_eq!(expected.0.len(), editor.term.cursor);
}

// `entries`: history entries before `keys` pressed
// `keys`: keys to press
// `expected`: line status before enter key: strings before and after cursor
fn assert_history(entries: &[&str], keys: &[KeyEvent], prompt: &str, expected: (&str, &str)) {
    let mut editor = init_editor(keys);
    for entry in entries {
        editor.history.add(entry).unwrap();
    }
    let actual_line = editor.readline(prompt).unwrap();
    assert_eq!(expected.0.to_owned() + expected.1, actual_line);
    if prompt.is_empty() {
        assert_eq!(expected.0.len(), editor.term.cursor);
    }
}

#[test]
fn unknown_esc_key() {
    assert_line(&[E(K::UnknownEscSeq, M::NONE), E::ENTER], "");
}

#[test]
fn test_send() {
    fn assert_send<T: Send>() {}
    assert_send::<DefaultEditor>();
}

#[test]
fn test_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<DefaultEditor>();
}

#[test]
fn test_apply_backspace_direct() {
    assert_eq!(
        &apply_backspace_direct("Hel\u{0008}\u{0008}el\u{0008}llo ☹\u{0008}☺"),
        "Hello ☺"
    );
}

#[test]
fn test_readline_direct() {
    use std::io::Cursor;

    let mut input = Cursor::new("([)\r\nab\u{0008}c\n\nlast".as_bytes());
    assert_eq!(readline_direct(&mut input).unwrap(), "([)");
    assert_eq!(readline_direct(&mut input).unwrap(), "ac");
    assert_eq!(readline_direct(&mut input).unwrap(), "");
    assert_eq!(readline_direct(&mut input).unwrap(), "last");
    assert!(matches!(
        readline_direct(&mut input),
        Err(crate::error::ReadlineError::Eof)
    ));
}

#[test]
fn test_auto_add_history() {
    let config = Config::builder().auto_add_history(true).build();
    let mut editor = DefaultEditor::with_config(config).unwrap();
    editor
        .term
        .keys
        .extend([E::from('a'), E::ENTER].iter().copied());
    assert_eq!("a", editor.readline(">>").unwrap());
    assert_eq!(1, editor.history().len());
}
