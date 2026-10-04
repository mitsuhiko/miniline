//! Basic commands tests.
use super::{assert_cursor, assert_line, assert_line_with_initial, init_editor};
use crate::error::ReadlineError;
use crate::keys::{KeyCode as K, KeyEvent as E, Modifiers as M};

#[test]
fn home_key() {
    assert_cursor(("", ""), &[E(K::Home, M::NONE), E::ENTER], ("", ""));
    assert_cursor(("Hi", ""), &[E(K::Home, M::NONE), E::ENTER], ("", "Hi"));
}

#[test]
fn end_key() {
    assert_cursor(("", ""), &[E(K::End, M::NONE), E::ENTER], ("", ""));
    assert_cursor(("H", "i"), &[E(K::End, M::NONE), E::ENTER], ("Hi", ""));
    assert_cursor(("", "Hi"), &[E(K::End, M::NONE), E::ENTER], ("Hi", ""));
}

#[test]
fn left_key() {
    assert_cursor(("Hi", ""), &[E(K::Left, M::NONE), E::ENTER], ("H", "i"));
    assert_cursor(("H", "i"), &[E(K::Left, M::NONE), E::ENTER], ("", "Hi"));
    assert_cursor(("", "Hi"), &[E(K::Left, M::NONE), E::ENTER], ("", "Hi"));
}

#[test]
fn right_key() {
    assert_cursor(("", ""), &[E(K::Right, M::NONE), E::ENTER], ("", ""));
    assert_cursor(("", "Hi"), &[E(K::Right, M::NONE), E::ENTER], ("H", "i"));
    assert_cursor(("B", "ye"), &[E(K::Right, M::NONE), E::ENTER], ("By", "e"));
    assert_cursor(("H", "i"), &[E(K::Right, M::NONE), E::ENTER], ("Hi", ""));
}

#[test]
fn enter_key() {
    assert_line(&[E::ENTER], "");
    assert_line(&[E::from('a'), E::ENTER], "a");
    assert_line_with_initial(("Hi", ""), &[E::ENTER], "Hi");
    assert_line_with_initial(("", "Hi"), &[E::ENTER], "Hi");
    assert_line_with_initial(("H", "i"), &[E::ENTER], "Hi");
}

#[test]
fn newline_key() {
    assert_line(&[E::ctrl('J')], "");
    assert_line(&[E::from('a'), E::ctrl('J')], "a");
}

#[test]
fn eof_key() {
    let mut editor = init_editor(&[E::ctrl('D')]);
    let err = editor.readline(">>");
    assert!(matches!(err, Err(ReadlineError::Eof)));
    assert_line(&[E::from('a'), E::ctrl('D'), E::ENTER], "a");
    assert_line_with_initial(("", "Hi"), &[E::ctrl('D'), E::ENTER], "i");
}

#[test]
fn interrupt_key() {
    let mut editor = init_editor(&[E::ctrl('C')]);
    let err = editor.readline(">>");
    assert!(matches!(err, Err(ReadlineError::Interrupted)));

    let mut editor = init_editor(&[E::ctrl('C')]);
    let err = editor.readline_with_initial(">>", ("Hi", ""));
    assert!(matches!(err, Err(ReadlineError::Interrupted)));
}

#[test]
fn delete_key() {
    assert_cursor(("a", ""), &[E(K::Delete, M::NONE), E::ENTER], ("a", ""));
    assert_cursor(("", "a"), &[E(K::Delete, M::NONE), E::ENTER], ("", ""));
}

#[test]
fn ctrl_t() {
    assert_cursor(("a", "b"), &[E::ctrl('T'), E::ENTER], ("ba", ""));
    assert_cursor(("ab", "cd"), &[E::ctrl('T'), E::ENTER], ("acb", "d"));
}

#[test]
fn ctrl_u() {
    assert_cursor(
        ("start of line ", "end"),
        &[E::ctrl('U'), E::ENTER],
        ("", "end"),
    );
    assert_cursor(("", "end"), &[E::ctrl('U'), E::ENTER], ("", "end"));
}

#[cfg(unix)]
#[test]
fn ctrl_v() {
    assert_cursor(
        ("", ""),
        &[E::ctrl('V'), E(K::Char('\t'), M::NONE), E::ENTER],
        ("\t", ""),
    );
}

#[test]
fn ctrl_w() {
    assert_cursor(
        ("Hello, ", "world"),
        &[E::ctrl('W'), E::ENTER],
        ("", "world"),
    );
    assert_cursor(
        ("Hello, world.", ""),
        &[E::ctrl('W'), E::ENTER],
        ("Hello, ", ""),
    );
}

#[test]
fn ctrl_y() {
    assert_cursor(
        ("Hello, ", "world"),
        &[E::ctrl('W'), E::ctrl('Y'), E::ENTER],
        ("Hello, ", "world"),
    );
}

#[test]
fn ctrl__() {
    assert_cursor(
        ("Hello, ", "world"),
        &[E::ctrl('W'), E::ctrl('_'), E::ENTER],
        ("Hello, ", "world"),
    );
}

#[test]
fn paste() {
    assert_cursor(
        ("", ""),
        &[E(K::BracketedPasteStart, M::NONE), E::ENTER],
        ("pasted", ""),
    );
}
