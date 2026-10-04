# miniline

A dependency free, minimal readline implementation for Rust.

`miniline` is a trimmed down fork of
[rustyline](https://github.com/kkawakam/rustyline) by Katsu Kawakami and the
Rustyline authors (which in turn is based on
[Antirez's Linenoise](https://github.com/antirez/linenoise)).  All the hard
work — the line buffer, the Emacs key map, the kill ring, the undo manager,
the escape sequence parser, the renderers and most of the tests — comes from
rustyline.  `miniline` keeps rustyline's API and behavior for the subset it
supports and removes everything else, including all dependencies.

```rust
use miniline::error::ReadlineError;
use miniline::DefaultEditor;

fn main() -> miniline::Result<()> {
    let mut rl = DefaultEditor::new()?;
    loop {
        match rl.readline(">> ") {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                println!("Line: {line}");
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}
```

## What's supported

* Unix (Linux, Android, macOS/iOS, FreeBSD, NetBSD, OpenBSD, DragonFly) and
  Windows consoles.  Platform APIs (termios, `ioctl`, `poll`, signals, the
  Win32 console API) are declared by hand, so there is no `libc`, `nix` or
  `windows-sys` dependency.  On other platforms, when `TERM` is
  unsupported or when stdin is not a terminal, lines are read without
  editing.
* Emacs mode key bindings (the rustyline defaults), including numeric
  arguments, word movement/killing, case changes, transposition, kill ring
  (`C-y`, `M-y`), undo (`C-_`, `C-x C-u`), quoted insert, suspend (`C-z`)
  and bracketed paste.
* An in-memory history (`DefaultHistory` / `MemHistory`) with history
  navigation and incremental search (`C-r`, `C-s`).
* Multi-line editing / line wrapping and terminal resizing.
* A subset of `Config` (`max_history_size`, `history_ignore_dups`,
  `history_ignore_space`, `auto_add_history`, `tab_stop`, `bracketed_paste`,
  `enable_signals`, synchronized output).

## What's not supported

Completion, hints, highlighting, validation, Vi mode, custom key bindings,
file / SQLite history, external printers and the derive macros.  If you need
any of these, use [rustyline](https://github.com/kkawakam/rustyline).

## Features

By default `miniline` has no dependencies and uses a compact built-in
approximation of Unicode grapheme segmentation and display widths.  For
accurate results enable the `unicode` feature, which pulls in
[`unicode-segmentation`](https://crates.io/crates/unicode-segmentation) and
[`unicode-width`](https://crates.io/crates/unicode-width) (they can also be
enabled individually with the `unicode-segmentation` and `unicode-width`
features).

## Migrating from rustyline

For code that only uses the supported subset, renaming the dependency is
enough:

```toml
[dependencies]
rustyline = { package = "miniline", version = "0.1" }
```

## License

MIT, same as rustyline.  See [LICENSE](LICENSE).
