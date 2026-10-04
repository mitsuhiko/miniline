//! History related commands tests
use super::assert_history;
use crate::keys::{KeyCode as K, KeyEvent as E, Modifiers as M};

#[test]
fn down_key() {
    assert_history(&["line1"], &[E(K::Down, M::NONE), E::ENTER], "", ("", ""));
    assert_history(
        &["line1", "line2"],
        &[
            E(K::Up, M::NONE),
            E(K::Up, M::NONE),
            E(K::Down, M::NONE),
            E::ENTER,
        ],
        "",
        ("line2", ""),
    );
    assert_history(
        &["line1"],
        &[
            E::from('a'),
            E(K::Up, M::NONE),
            E(K::Down, M::NONE), // restore original line
            E::ENTER,
        ],
        "",
        ("a", ""),
    );
    assert_history(
        &["line1"],
        &[
            E::from('a'),
            E(K::Down, M::NONE), // noop
            E::ENTER,
        ],
        "",
        ("a", ""),
    );
}

#[test]
fn up_key() {
    assert_history(&[], &[E(K::Up, M::NONE), E::ENTER], "", ("", ""));
    assert_history(
        &["line1"],
        &[E(K::Up, M::NONE), E::ENTER],
        "",
        ("line1", ""),
    );
    assert_history(
        &["line1", "line2"],
        &[E(K::Up, M::NONE), E(K::Up, M::NONE), E::ENTER],
        "",
        ("line1", ""),
    );
}

#[test]
fn ctrl_r() {
    assert_history(&[], &[E::ctrl('R'), E::from('o'), E::ENTER], "", ("o", ""));
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('o'),
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("cargo", ""),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('u'),
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("ru", "stc"),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('r'),
            E::from('u'),
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("r", "ustc"),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('r'),
            E::ctrl('R'),
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("r", "ustc"),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('r'),
            E::from('z'),         // no match
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("car", "go"),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::from('a'),
            E::ctrl('R'),
            E::from('r'),
            E::ctrl('G'), // abort
            E::ENTER,
        ],
        "",
        ("a", ""),
    );
}

#[test]
fn ctrl_r_with_long_prompt() {
    assert_history(
        &["rustc", "cargo"],
        &[E::ctrl('R'), E::from('o'), E::ENTER],
        ">>>>>>>>>>>>>>>>>>>>>>>>>>> ",
        ("cargo", ""),
    );
}

#[test]
fn ctrl_s() {
    assert_history(
        &["rustc", "cargo"],
        &[
            E::ctrl('R'),
            E::from('r'),
            E::ctrl('R'),
            E::ctrl('S'),
            E(K::Right, M::NONE), // just to assert cursor pos
            E::ENTER,
        ],
        "",
        ("car", "go"),
    );
}

#[test]
fn meta_lt() {
    assert_history(&[""], &[E::alt('<'), E::ENTER], "", ("", ""));
    assert_history(
        &["rustc", "cargo"],
        &[E::alt('<'), E::ENTER],
        "",
        ("rustc", ""),
    );
}

#[test]
fn meta_gt() {
    assert_history(&[""], &[E::alt('>'), E::ENTER], "", ("", ""));
    assert_history(
        &["rustc", "cargo"],
        &[E::alt('<'), E::alt('>'), E::ENTER],
        "",
        ("", ""),
    );
    assert_history(
        &["rustc", "cargo"],
        &[
            E::from('a'),
            E::alt('<'),
            E::alt('>'), // restore original line
            E::ENTER,
        ],
        "",
        ("a", ""),
    );
}
