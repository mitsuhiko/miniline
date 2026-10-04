//! Unix specific definitions
//!
//! This talks to libc directly (no `libc` / `nix` crates).  The termios
//! structure is treated as an opaque blob with per-platform field offsets and
//! flag values.  Platforms where these are not known fall back to the
//! "unsupported terminal" mode (plain line reading).
use std::cmp;
use std::collections::HashMap;
use std::io::{self, ErrorKind, IsTerminal, Read};
use std::os::raw::{c_int, c_void};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicI32, Ordering};

use super::{width, RawMode, RawReader, Renderer, Term};
use crate::config::Config;
use crate::error::Signal;
use crate::keymap::Cmd;
use crate::keys::{KeyCode as K, KeyEvent, KeyEvent as E, Modifiers as M};
use crate::layout::{GraphemeClusterMode, Layout, Position, Unit};
use crate::line_buffer::LineBuffer;
use crate::unicode::graphemes;
use crate::{ReadlineError, Result};

const BRACKETED_PASTE_ON: &str = "\x1b[?2004h";
const BRACKETED_PASTE_OFF: &str = "\x1b[?2004l";
const BEGIN_SYNCHRONIZED_UPDATE: &str = "\x1b[?2026h";
const END_SYNCHRONIZED_UPDATE: &str = "\x1b[?2026l";

macro_rules! linux_like {
    ($($item:item)*) => {$(
        #[cfg(all(
            any(target_os = "linux", target_os = "android"),
            any(
                target_arch = "x86",
                target_arch = "x86_64",
                target_arch = "arm",
                target_arch = "aarch64",
                target_arch = "riscv32",
                target_arch = "riscv64",
                target_arch = "loongarch64",
                target_arch = "s390x"
            )
        ))]
        $item
    )*};
}

macro_rules! bsd_like {
    ($($item:item)*) => {$(
        #[cfg(any(
            target_vendor = "apple",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        $item
    )*};
}

macro_rules! unknown_platform {
    ($($item:item)*) => {$(
        #[cfg(not(any(
            all(
                any(target_os = "linux", target_os = "android"),
                any(
                    target_arch = "x86",
                    target_arch = "x86_64",
                    target_arch = "arm",
                    target_arch = "aarch64",
                    target_arch = "riscv32",
                    target_arch = "riscv64",
                    target_arch = "loongarch64",
                    target_arch = "s390x"
                )
            ),
            target_vendor = "apple",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        )))]
        $item
    )*};
}

/// Platform specific constants.
#[allow(dead_code)]
mod sys {
    use std::os::raw::c_int;

    linux_like! {
        pub const SUPPORTED: bool = true;
        pub type TcFlag = std::os::raw::c_uint;
        pub const CC_OFFSET: usize = 17;
        pub const BRKINT: TcFlag = 0o2;
        pub const ICRNL: TcFlag = 0o400;
        pub const INPCK: TcFlag = 0o20;
        pub const ISTRIP: TcFlag = 0o40;
        pub const IXON: TcFlag = 0o2000;
        pub const CS8: TcFlag = 0o60;
        pub const ECHO: TcFlag = 0o10;
        pub const ICANON: TcFlag = 0o2;
        pub const IEXTEN: TcFlag = 0o100000;
        pub const ISIG: TcFlag = 0o1;
        pub const VINTR: usize = 0;
        pub const VQUIT: usize = 1;
        pub const VEOF: usize = 4;
        pub const VTIME: usize = 5;
        pub const VMIN: usize = 6;
        pub const VSUSP: usize = 10;
        pub const SIGTSTP: c_int = 20;
        pub type NfdsT = std::os::raw::c_ulong;
        #[cfg(any(target_env = "musl", target_os = "android"))]
        pub type IoctlReq = c_int;
        #[cfg(not(any(target_env = "musl", target_os = "android")))]
        pub type IoctlReq = std::os::raw::c_ulong;
        pub const TIOCGWINSZ: IoctlReq = 0x5413;
    }

    bsd_like! {
        pub const SUPPORTED: bool = true;
        #[cfg(target_vendor = "apple")]
        pub type TcFlag = std::os::raw::c_ulong;
        #[cfg(not(target_vendor = "apple"))]
        pub type TcFlag = std::os::raw::c_uint;
        pub const CC_OFFSET: usize = 4 * std::mem::size_of::<TcFlag>();
        pub const BRKINT: TcFlag = 0x2;
        pub const ICRNL: TcFlag = 0x100;
        pub const INPCK: TcFlag = 0x10;
        pub const ISTRIP: TcFlag = 0x20;
        pub const IXON: TcFlag = 0x200;
        pub const CS8: TcFlag = 0x300;
        pub const ECHO: TcFlag = 0x8;
        pub const ICANON: TcFlag = 0x100;
        pub const IEXTEN: TcFlag = 0x400;
        pub const ISIG: TcFlag = 0x80;
        pub const VEOF: usize = 0;
        pub const VINTR: usize = 8;
        pub const VQUIT: usize = 9;
        pub const VSUSP: usize = 10;
        pub const VMIN: usize = 16;
        pub const VTIME: usize = 17;
        pub const SIGTSTP: c_int = 18;
        pub type NfdsT = std::os::raw::c_uint;
        pub type IoctlReq = std::os::raw::c_ulong;
        pub const TIOCGWINSZ: IoctlReq = 0x40087468;
    }

    unknown_platform! {
        pub const SUPPORTED: bool = false;
        pub type TcFlag = std::os::raw::c_uint;
        pub const CC_OFFSET: usize = 16;
        pub const BRKINT: TcFlag = 0;
        pub const ICRNL: TcFlag = 0;
        pub const INPCK: TcFlag = 0;
        pub const ISTRIP: TcFlag = 0;
        pub const IXON: TcFlag = 0;
        pub const CS8: TcFlag = 0;
        pub const ECHO: TcFlag = 0;
        pub const ICANON: TcFlag = 0;
        pub const IEXTEN: TcFlag = 0;
        pub const ISIG: TcFlag = 0;
        pub const VEOF: usize = 0;
        pub const VINTR: usize = 0;
        pub const VQUIT: usize = 0;
        pub const VSUSP: usize = 0;
        pub const VMIN: usize = 0;
        pub const VTIME: usize = 0;
        pub const SIGTSTP: c_int = 0;
        pub type NfdsT = std::os::raw::c_uint;
        pub type IoctlReq = std::os::raw::c_ulong;
        pub const TIOCGWINSZ: IoctlReq = 0;
    }

    // These are the same on all supported platforms
    pub const TCSADRAIN: c_int = 1;
    pub const SIGINT: c_int = 2;
    pub const SIGWINCH: c_int = 28;
    pub const POLLIN: i16 = 1;
    pub const ENOTTY: i32 = 25;
    /// Offset of the handler in `struct sigaction`.  It is the first field
    /// everywhere except for 64-bit bionic where `sa_flags` comes first.
    #[cfg(all(target_os = "android", target_pointer_width = "64"))]
    pub const SA_HANDLER_OFFSET: usize = 8;
    #[cfg(not(all(target_os = "android", target_pointer_width = "64")))]
    pub const SA_HANDLER_OFFSET: usize = 0;
}

mod ffi {
    use std::os::raw::{c_int, c_void};

    use super::sys::{IoctlReq, NfdsT};

    #[repr(C)]
    pub struct PollFd {
        pub fd: c_int,
        pub events: i16,
        pub revents: i16,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct WinSize {
        pub ws_row: u16,
        pub ws_col: u16,
        pub ws_xpixel: u16,
        pub ws_ypixel: u16,
    }

    /// Opaque, generously sized `struct sigaction`.  Zeroed it represents
    /// an empty signal mask and no flags.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct SigAction(pub [u8; 256]);

    /// Opaque, generously sized `struct termios`.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct Termios(pub [u8; 256]);

    extern "C" {
        pub fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
        pub fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        pub fn poll(fds: *mut PollFd, nfds: NfdsT, timeout: c_int) -> c_int;
        pub fn tcgetattr(fd: c_int, termios: *mut Termios) -> c_int;
        pub fn tcsetattr(fd: c_int, optional_actions: c_int, termios: *const Termios) -> c_int;
        pub fn ioctl(fd: c_int, request: IoctlReq, ...) -> c_int;
        pub fn sigaction(signum: c_int, act: *const SigAction, oldact: *mut SigAction) -> c_int;
        #[allow(dead_code)]
        pub fn kill(pid: i32, sig: c_int) -> c_int;
    }
}

use self::ffi::{PollFd, Termios};

impl Termios {
    fn flag(&self, idx: usize) -> sys::TcFlag {
        let off = idx * std::mem::size_of::<sys::TcFlag>();
        // SAFETY: offset is in bounds of the buffer
        unsafe { std::ptr::read_unaligned(self.0.as_ptr().add(off).cast::<sys::TcFlag>()) }
    }

    fn set_flag(&mut self, idx: usize, value: sys::TcFlag) {
        let off = idx * std::mem::size_of::<sys::TcFlag>();
        // SAFETY: offset is in bounds of the buffer
        unsafe {
            std::ptr::write_unaligned(self.0.as_mut_ptr().add(off).cast::<sys::TcFlag>(), value);
        }
    }

    fn cc(&self, idx: usize) -> u8 {
        self.0[sys::CC_OFFSET + idx]
    }

    fn set_cc(&mut self, idx: usize, value: u8) {
        self.0[sys::CC_OFFSET + idx] = value;
    }
}

const IFLAG: usize = 0;
const CFLAG: usize = 2;
const LFLAG: usize = 3;

fn get_win_size(fd: RawFd) -> (Unit, Unit) {
    if cfg!(test) || !sys::SUPPORTED {
        return (80, 24);
    }

    let mut size = ffi::WinSize::default();
    // SAFETY: TIOCGWINSZ writes a `struct winsize`
    match unsafe { ffi::ioctl(fd, sys::TIOCGWINSZ, &mut size as *mut ffi::WinSize) } {
        0 => {
            // In linux pseudo-terminals are created with dimensions of
            // zero. If host application didn't initialize the correct
            // size before start we treat zero size as 80 columns and
            // infinite rows
            let cols = if size.ws_col == 0 { 80 } else { size.ws_col };
            let rows = if size.ws_row == 0 {
                Unit::MAX
            } else {
                size.ws_row
            };
            (cols, rows)
        }
        _ => (80, 24),
    }
}

pub type PosixBuffer = Vec<u8>;

pub type PosixKeyMap = HashMap<KeyEvent, Cmd>;
#[cfg(not(test))]
pub type KeyMap = PosixKeyMap;

#[must_use = "You must restore default mode (disable_raw_mode)"]
pub struct PosixMode {
    termios: Termios,
    tty_in: RawFd,
    tty_out: Option<RawFd>,
}

#[cfg(not(test))]
pub type Mode = PosixMode;

impl RawMode for PosixMode {
    /// Disable RAW mode for the terminal.
    fn disable_raw_mode(&self) -> Result<()> {
        termios_::disable_raw_mode(self.tty_in, &self.termios)?;
        // disable bracketed paste
        if let Some(out) = self.tty_out {
            write_all(out, BRACKETED_PASTE_OFF)?;
        }
        Ok(())
    }
}

const READ_BUFFER_SIZE: usize = 1024;

/// Console input reader
///
/// Rust std::io::Stdin is buffered with no way to know if bytes are
/// available. So we use low-level stuff instead...
pub struct PosixRawReader {
    fd: RawFd,
    sig: Option<Sig>,
    buf: Vec<u8>,
    pos: usize,
    key_map: PosixKeyMap,
}

const UP: char = 'A'; // kcuu1, kUP*
const DOWN: char = 'B'; // kcud1, kDN*
const RIGHT: char = 'C'; // kcuf1, kRIT*
const LEFT: char = 'D'; // kcub1, kLFT*
const END: char = 'F'; // kend*
const HOME: char = 'H'; // khom*
const INSERT: char = '2'; // kic*
const DELETE: char = '3'; // kdch1, kDC*
const PAGE_UP: char = '5'; // kpp, kPRV*
const PAGE_DOWN: char = '6'; // knp, kNXT*

const RXVT_HOME: char = '7';
const RXVT_END: char = '8';

const SHIFT: char = '2';
const ALT: char = '3';
const ALT_SHIFT: char = '4';
const CTRL: char = '5';
const CTRL_SHIFT: char = '6';
const CTRL_ALT: char = '7';
const CTRL_ALT_SHIFT: char = '8';

const RXVT_SHIFT: char = '$';
const RXVT_CTRL: char = '\x1e';
const RXVT_CTRL_SHIFT: char = '@';

/// xterm style modifier parameter (`2` .. `8`)
fn modifiers(c: char) -> Option<M> {
    Some(match c {
        SHIFT => M::SHIFT,
        ALT => M::ALT,
        ALT_SHIFT => M::ALT_SHIFT,
        CTRL => M::CTRL,
        CTRL_SHIFT => M::CTRL_SHIFT,
        CTRL_ALT => M::CTRL_ALT,
        CTRL_ALT_SHIFT => M::CTRL_ALT_SHIFT,
        _ => return None,
    })
}

impl PosixRawReader {
    fn new(fd: RawFd, sig: Option<Sig>, buffer: Option<PosixBuffer>, key_map: PosixKeyMap) -> Self {
        Self {
            fd,
            sig,
            buf: buffer.unwrap_or_default(),
            pos: 0,
            key_map,
        }
    }

    fn buffered(&self) -> bool {
        self.pos < self.buf.len()
    }

    /// Check if a signal has been received
    fn sig(&self) -> io::Result<Option<Signal>> {
        if let Some(ref sig) = self.sig {
            let mut buf = [0u8; 64];
            match (&sig.pipe).read(&mut buf) {
                Ok(0) => Ok(None),
                Ok(_) => Ok(Some(match buf[0] {
                    b'I' => Signal::Interrupt,
                    _ => Signal::Resize,
                })),
                Err(e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted =>
                {
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        }
    }

    /// Blocking read of the next chunk of input into the buffer.
    fn fill_buf(&mut self) -> Result<()> {
        loop {
            if let Some(ref sig) = self.sig {
                // wait for either input or a signal notification
                let mut fds = [
                    PollFd {
                        fd: self.fd,
                        events: sys::POLLIN,
                        revents: 0,
                    },
                    PollFd {
                        fd: sig.pipe.as_raw_fd(),
                        events: sys::POLLIN,
                        revents: 0,
                    },
                ];
                // SAFETY: fds is a valid array of 2 pollfd
                let rc = unsafe { ffi::poll(fds.as_mut_ptr(), 2, -1) };
                if rc < 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() != ErrorKind::Interrupted {
                        return Err(err.into());
                    }
                }
                if let Some(signal) = self.sig()? {
                    return Err(ReadlineError::Signal(signal));
                }
                if rc <= 0 || fds[0].revents == 0 {
                    continue;
                }
            }
            let mut buf = [0u8; READ_BUFFER_SIZE];
            // SAFETY: buf is valid for `buf.len()` bytes
            let res = unsafe { ffi::read(self.fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
            if res < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == ErrorKind::Interrupted {
                    if let Some(signal) = self.sig()? {
                        return Err(ReadlineError::Signal(signal));
                    }
                    continue;
                }
                return Err(err.into());
            } else if res == 0 {
                return Err(ReadlineError::Eof);
            }
            #[allow(clippy::cast_sign_loss)]
            let n = res as usize;
            self.buf.clear();
            self.buf.extend_from_slice(&buf[..n]);
            self.pos = 0;
            return Ok(());
        }
    }

    fn next_byte(&mut self) -> Result<u8> {
        if !self.buffered() {
            self.fill_buf()?;
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Handle \E <seq1> sequences
    // https://invisible-island.net/xterm/xterm-function-keys.html
    fn escape_sequence(&mut self) -> Result<KeyEvent> {
        self.do_escape_sequence(true)
    }

    /// Don't call directly, call `PosixRawReader::escape_sequence` instead
    fn do_escape_sequence(&mut self, allow_recurse: bool) -> Result<KeyEvent> {
        // Read the next byte representing the escape sequence.
        let seq1 = self.next_char()?;
        if seq1 == '[' {
            // \E[ sequences. (CSI)
            self.escape_csi()
        } else if seq1 == 'O' {
            // xterm
            // \EO sequences. (SS3)
            self.escape_o()
        } else if seq1 == '\x1b' {
            // \E\E — used by rxvt, iTerm (under default config), etc.
            // ```
            // \E\E[A => Alt-Up
            // \E\E[B => Alt-Down
            // \E\E[C => Alt-Right
            // \E\E[D => Alt-Left
            // ```
            //
            // In general this more or less works just adding ALT to an existing
            // key, but has a wrinkle in that `ESC ESC` without anything
            // following should be interpreted as the escape key.
            //
            // We handle this by polling to see if there's anything coming
            // within our timeout, and if so, recursing once, but adding alt to
            // what we read.
            if !allow_recurse {
                return Ok(E::ESC);
            }
            match self.poll(100) {
                // Ignore poll errors, it's very likely we'll pick them up on
                // the next read anyway.
                Ok(false) | Err(_) => Ok(E::ESC),
                Ok(true) => {
                    // recurse, and add the alt modifier.
                    let E(k, m) = self.do_escape_sequence(false)?;
                    Ok(E(k, m | M::ALT))
                }
            }
        } else {
            Ok(E::alt(seq1))
        }
    }

    /// Handle \E[ <seq2> escape sequences
    fn escape_csi(&mut self) -> Result<KeyEvent> {
        let seq2 = self.next_char()?;
        if seq2.is_ascii_digit() {
            match seq2 {
                '0' | '9' => Ok(E(K::UnknownEscSeq, M::NONE)),
                _ => {
                    // Extended escape, read additional byte.
                    self.extended_escape(seq2)
                }
            }
        } else if seq2 == '[' {
            let seq3 = self.next_char()?;
            // Linux console
            Ok(match seq3 {
                'A' => E(K::F(1), M::NONE),
                'B' => E(K::F(2), M::NONE),
                'C' => E(K::F(3), M::NONE),
                'D' => E(K::F(4), M::NONE),
                'E' => E(K::F(5), M::NONE),
                _ => E(K::UnknownEscSeq, M::NONE),
            })
        } else {
            // ANSI
            Ok(match seq2 {
                UP => E(K::Up, M::NONE),
                DOWN => E(K::Down, M::NONE),
                RIGHT => E(K::Right, M::NONE),
                LEFT => E(K::Left, M::NONE),
                END => E(K::End, M::NONE),
                HOME => E(K::Home, M::NONE), // khome
                'Z' => E(K::BackTab, M::NONE),
                'a' => E(K::Up, M::SHIFT),    // rxvt: kind or kUP
                'b' => E(K::Down, M::SHIFT),  // rxvt: kri or kDN
                'c' => E(K::Right, M::SHIFT), // rxvt
                'd' => E(K::Left, M::SHIFT),  // rxvt
                _ => E(K::UnknownEscSeq, M::NONE),
            })
        }
    }

    /// Handle \E[ <seq2:digit> escape sequences
    fn extended_escape(&mut self, seq2: char) -> Result<KeyEvent> {
        let seq3 = self.next_char()?;
        if seq3 == '~' {
            Ok(match seq2 {
                '1' | RXVT_HOME => E(K::Home, M::NONE), // tmux, xrvt
                INSERT => E(K::Insert, M::NONE),
                DELETE => E(K::Delete, M::NONE),
                '4' | RXVT_END => E(K::End, M::NONE), // tmux, xrvt
                PAGE_UP => E(K::PageUp, M::NONE),
                PAGE_DOWN => E(K::PageDown, M::NONE),
                _ => E(K::UnknownEscSeq, M::NONE),
            })
        } else if seq3.is_ascii_digit() {
            let seq4 = self.next_char()?;
            if seq4 == '~' {
                Ok(match (seq2, seq3) {
                    ('1', '1') => E(K::F(1), M::NONE),  // rxvt-unicode
                    ('1', '2') => E(K::F(2), M::NONE),  // rxvt-unicode
                    ('1', '3') => E(K::F(3), M::NONE),  // rxvt-unicode
                    ('1', '4') => E(K::F(4), M::NONE),  // rxvt-unicode
                    ('1', '5') => E(K::F(5), M::NONE),  // kf5
                    ('1', '7') => E(K::F(6), M::NONE),  // kf6
                    ('1', '8') => E(K::F(7), M::NONE),  // kf7
                    ('1', '9') => E(K::F(8), M::NONE),  // kf8
                    ('2', '0') => E(K::F(9), M::NONE),  // kf9
                    ('2', '1') => E(K::F(10), M::NONE), // kf10
                    ('2', '3') => E(K::F(11), M::NONE), // kf11
                    ('2', '4') => E(K::F(12), M::NONE), // kf12
                    _ => E(K::UnknownEscSeq, M::NONE),
                })
            } else if seq4 == ';' {
                let seq5 = self.next_char()?;
                if seq5.is_ascii_digit() {
                    let seq6 = self.next_char()?;
                    if seq6.is_ascii_digit() {
                        self.next_char()?; // 'R' expected
                        Ok(E(K::UnknownEscSeq, M::NONE))
                    } else if seq6 == 'R' {
                        Ok(E(K::UnknownEscSeq, M::NONE))
                    } else if seq6 == '~' {
                        Ok(match (seq2, seq3, seq5) {
                            ('1', '5', CTRL) => E(K::F(5), M::CTRL),
                            ('1', '7', CTRL) => E(K::F(6), M::CTRL),
                            ('1', '8', CTRL) => E(K::F(7), M::CTRL),
                            ('1', '9', CTRL) => E(K::F(8), M::CTRL),
                            ('2', '0', CTRL) => E(K::F(9), M::CTRL),
                            ('2', '1', CTRL) => E(K::F(10), M::CTRL),
                            ('2', '3', CTRL) => E(K::F(11), M::CTRL),
                            ('2', '4', CTRL) => E(K::F(12), M::CTRL),
                            _ => E(K::UnknownEscSeq, M::NONE),
                        })
                    } else {
                        Ok(E(K::UnknownEscSeq, M::NONE))
                    }
                } else {
                    Ok(E(K::UnknownEscSeq, M::NONE))
                }
            } else if seq4.is_ascii_digit() {
                let seq5 = self.next_char()?;
                if seq5 == '~' {
                    Ok(match (seq2, seq3, seq4) {
                        ('2', '0', '0') => E(K::BracketedPasteStart, M::NONE),
                        ('2', '0', '1') => E(K::BracketedPasteEnd, M::NONE),
                        _ => E(K::UnknownEscSeq, M::NONE),
                    })
                } else {
                    Ok(E(K::UnknownEscSeq, M::NONE))
                }
            } else {
                Ok(E(K::UnknownEscSeq, M::NONE))
            }
        } else if seq3 == ';' {
            let seq4 = self.next_char()?;
            if seq4.is_ascii_digit() {
                let seq5 = self.next_char()?;
                if seq5.is_ascii_digit() {
                    self.next_char()?; // 'R' expected
                    Ok(E(K::UnknownEscSeq, M::NONE))
                } else if seq2 == '1' {
                    Ok(Self::modified_key(seq4, seq5))
                } else if seq5 == '~' {
                    let key = match seq2 {
                        INSERT => Some(K::Insert),
                        DELETE => Some(K::Delete),
                        PAGE_UP => Some(K::PageUp),
                        PAGE_DOWN => Some(K::PageDown),
                        _ => None,
                    };
                    Ok(match (key, modifiers(seq4)) {
                        (Some(key), Some(mods)) => E(key, mods),
                        _ => E(K::UnknownEscSeq, M::NONE),
                    })
                } else {
                    Ok(E(K::UnknownEscSeq, M::NONE))
                }
            } else {
                Ok(E(K::UnknownEscSeq, M::NONE))
            }
        } else {
            Ok(match (seq2, seq3) {
                (DELETE, RXVT_CTRL) => E(K::Delete, M::CTRL),
                (DELETE, RXVT_CTRL_SHIFT) => E(K::Delete, M::CTRL_SHIFT),
                (CTRL, UP) => E(K::Up, M::CTRL),
                (CTRL, DOWN) => E(K::Down, M::CTRL),
                (CTRL, RIGHT) => E(K::Right, M::CTRL),
                (CTRL, LEFT) => E(K::Left, M::CTRL),
                (PAGE_UP, RXVT_CTRL) => E(K::PageUp, M::CTRL),
                (PAGE_UP, RXVT_SHIFT) => E(K::PageUp, M::SHIFT),
                (PAGE_UP, RXVT_CTRL_SHIFT) => E(K::PageUp, M::CTRL_SHIFT),
                (PAGE_DOWN, RXVT_CTRL) => E(K::PageDown, M::CTRL),
                (PAGE_DOWN, RXVT_SHIFT) => E(K::PageDown, M::SHIFT),
                (PAGE_DOWN, RXVT_CTRL_SHIFT) => E(K::PageDown, M::CTRL_SHIFT),
                (RXVT_HOME, RXVT_CTRL) => E(K::Home, M::CTRL),
                (RXVT_HOME, RXVT_SHIFT) => E(K::Home, M::SHIFT),
                (RXVT_HOME, RXVT_CTRL_SHIFT) => E(K::Home, M::CTRL_SHIFT),
                (RXVT_END, RXVT_CTRL) => E(K::End, M::CTRL), // kEND5 or kel
                (RXVT_END, RXVT_SHIFT) => E(K::End, M::SHIFT),
                (RXVT_END, RXVT_CTRL_SHIFT) => E(K::End, M::CTRL_SHIFT),
                _ => E(K::UnknownEscSeq, M::NONE),
            })
        }
    }

    /// Handle \E[1; <seq4> <seq5> (xterm modified keys)
    fn modified_key(seq4: char, seq5: char) -> KeyEvent {
        if seq4 == '9' {
            // Meta + arrow on (some?) Macs when using iTerm defaults
            return match seq5 {
                UP => E(K::Up, M::ALT),
                DOWN => E(K::Down, M::ALT),
                RIGHT => E(K::Right, M::ALT),
                LEFT => E(K::Left, M::ALT),
                _ => E(K::UnknownEscSeq, M::NONE),
            };
        }
        let Some(mods) = modifiers(seq4) else {
            return E(K::UnknownEscSeq, M::NONE);
        };
        match seq5 {
            UP => E(K::Up, mods),     // ~ key_sr
            DOWN => E(K::Down, mods), // ~ key_sf
            RIGHT => E(K::Right, mods),
            LEFT => E(K::Left, mods),
            END => E(K::End, mods),   // kEND
            HOME => E(K::Home, mods), // kHOM
            'P' if mods == M::CTRL => E(K::F(1), M::CTRL),
            'Q' if mods == M::CTRL => E(K::F(2), M::CTRL),
            'S' if mods == M::CTRL => E(K::F(4), M::CTRL),
            // Ctrl + digits
            'p'..='y' if mods.contains(M::CTRL) => {
                E(K::Char((b'0' + (seq5 as u8 - b'p')) as char), mods)
            }
            _ => E(K::UnknownEscSeq, M::NONE),
        }
    }

    /// Handle \EO <seq2> escape sequences
    fn escape_o(&mut self) -> Result<KeyEvent> {
        let seq2 = self.next_char()?;
        Ok(match seq2 {
            UP => E(K::Up, M::NONE),
            DOWN => E(K::Down, M::NONE),
            RIGHT => E(K::Right, M::NONE),
            LEFT => E(K::Left, M::NONE),
            END => E(K::End, M::NONE),   // kend
            HOME => E(K::Home, M::NONE), // khome
            'M' => E::ENTER,             // kent
            'P' => E(K::F(1), M::NONE),  // kf1
            'Q' => E(K::F(2), M::NONE),  // kf2
            'R' => E(K::F(3), M::NONE),  // kf3
            'S' => E(K::F(4), M::NONE),  // kf4
            'a' => E(K::Up, M::CTRL),
            'b' => E(K::Down, M::CTRL),
            'c' => E(K::Right, M::CTRL), // rxvt
            'd' => E(K::Left, M::CTRL),  // rxvt
            'l' => E(K::F(8), M::NONE),
            't' => E(K::F(5), M::NONE),  // kf5 or kb1
            'u' => E(K::F(6), M::NONE),  // kf6 or kb2
            'v' => E(K::F(7), M::NONE),  // kf7 or kb3
            'w' => E(K::F(9), M::NONE),  // kf9 or ka1
            'x' => E(K::F(10), M::NONE), // kf10 or ka2
            _ => E(K::UnknownEscSeq, M::NONE),
        })
    }

    /// Wait at most `timeout_ms` (`-1` for infinite) for input.
    fn poll(&mut self, timeout_ms: c_int) -> Result<bool> {
        if self.buffered() {
            return Ok(true);
        }
        let mut fds = [PollFd {
            fd: self.fd,
            events: sys::POLLIN,
            revents: 0,
        }];
        // SAFETY: fds is a valid array of 1 pollfd
        let rc = unsafe { ffi::poll(fds.as_mut_ptr(), 1, timeout_ms) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                if let Some(signal) = self.sig()? {
                    Err(ReadlineError::Signal(signal))
                } else {
                    Ok(false) // Ignore EINTR while polling
                }
            } else {
                Err(err.into())
            }
        } else {
            Ok(rc != 0)
        }
    }
}

impl RawReader for PosixRawReader {
    type Buffer = PosixBuffer;

    fn next_key(&mut self, single_esc_abort: bool) -> Result<KeyEvent> {
        let c = self.next_char()?;

        let mut key = KeyEvent::new(c, M::NONE);
        if key == E::ESC {
            // There is no key sequence timeout in emacs mode: an escape is
            // a meta prefix unless a single escape is expected to abort.
            let timeout_ms = if single_esc_abort { 0 } else { -1 };
            match self.poll(timeout_ms) {
                Ok(false) => {
                    // single escape
                }
                Ok(_) => {
                    // escape sequence
                    key = self.escape_sequence()?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(key)
    }

    fn next_char(&mut self) -> Result<char> {
        let invalid = || ReadlineError::from(ErrorKind::InvalidData);
        let b0 = self.next_byte()?;
        let (len, mut cp) = match b0 {
            0x00..=0x7f => return Ok(char::from(b0)),
            0xc2..=0xdf => (2, u32::from(b0 & 0x1f)),
            0xe0..=0xef => (3, u32::from(b0 & 0x0f)),
            0xf0..=0xf4 => (4, u32::from(b0 & 0x07)),
            _ => return Err(invalid()),
        };
        for _ in 1..len {
            let b = self.next_byte()?;
            if b & 0xc0 != 0x80 {
                return Err(invalid());
            }
            cp = (cp << 6) | u32::from(b & 0x3f);
        }
        let min = match len {
            3 => 0x800,
            4 => 0x10000,
            _ => 0x80,
        };
        if cp < min {
            return Err(invalid());
        }
        char::from_u32(cp).ok_or_else(invalid)
    }

    fn read_pasted_text(&mut self) -> Result<String> {
        let mut buffer = String::new();
        loop {
            match self.next_char()? {
                '\x1b' => {
                    let key = self.escape_sequence()?;
                    if key == E(K::BracketedPasteEnd, M::NONE) {
                        break;
                    }
                    continue; // TODO validate
                }
                c => buffer.push(c),
            }
        }
        let buffer = buffer.replace("\r\n", "\n");
        let buffer = buffer.replace('\r', "\n");
        Ok(buffer)
    }

    fn find_binding(&self, key: &KeyEvent) -> Option<Cmd> {
        self.key_map.get(key).cloned()
    }

    fn unbuffer(self) -> Option<PosixBuffer> {
        if self.buffered() {
            Some(self.buf[self.pos..].to_vec())
        } else {
            None
        }
    }
}

/// Console output writer
pub struct PosixRenderer {
    out: RawFd,
    cols: Unit, // Number of columns in terminal
    buffer: String,
    tab_stop: Unit,
    enable_synchronized_output: bool,
    grapheme_cluster_mode: GraphemeClusterMode,
    /// 0 when BSU is first used or after last ESU
    synchronized_update: usize,
}

impl PosixRenderer {
    fn new(
        out: RawFd,
        tab_stop: Unit,
        enable_synchronized_output: bool,
        grapheme_cluster_mode: GraphemeClusterMode,
    ) -> Self {
        let (cols, _) = get_win_size(out);
        Self {
            out,
            cols,
            buffer: String::with_capacity(1024),
            tab_stop,
            enable_synchronized_output,
            grapheme_cluster_mode,
            synchronized_update: 0,
        }
    }

    fn clear_old_rows(&mut self, layout: &Layout) {
        use std::fmt::Write as _;
        let current_row = layout.cursor.row;
        let old_rows = layout.end.row;
        // old_rows < cursor_row if the prompt spans multiple lines and if
        // this is the default State.
        let cursor_row_movement = old_rows.saturating_sub(current_row);
        // move the cursor down as required
        if cursor_row_movement > 0 {
            write!(self.buffer, "\x1b[{cursor_row_movement}B").unwrap();
        }
        // clear old rows
        for _ in 0..old_rows {
            self.buffer.push_str("\r\x1b[K\x1b[A");
        }
        // clear the line
        self.buffer.push_str("\r\x1b[K");
    }

    fn begin_synchronized_update(&mut self) -> Result<()> {
        if self.enable_synchronized_output {
            if self.synchronized_update == 0 {
                self.write_and_flush(BEGIN_SYNCHRONIZED_UPDATE)?;
            }
            self.synchronized_update = self.synchronized_update.saturating_add(1);
        }
        Ok(())
    }

    fn end_synchronized_update(&mut self) -> Result<()> {
        if self.enable_synchronized_output {
            self.synchronized_update = self.synchronized_update.saturating_sub(1);
            if self.synchronized_update == 0 {
                self.write_and_flush(END_SYNCHRONIZED_UPDATE)?;
            }
        }
        Ok(())
    }
}

impl Renderer for PosixRenderer {
    type Reader = PosixRawReader;

    fn move_cursor(&mut self, old: Position, new: Position) -> Result<()> {
        use std::fmt::Write as _;
        self.buffer.clear();
        let row_ordering = new.row.cmp(&old.row);
        if row_ordering == cmp::Ordering::Greater {
            // move down
            let row_shift = new.row - old.row;
            if row_shift == 1 {
                self.buffer.push_str("\x1b[B");
            } else {
                write!(self.buffer, "\x1b[{row_shift}B")?;
            }
        } else if row_ordering == cmp::Ordering::Less {
            // move up
            let row_shift = old.row - new.row;
            if row_shift == 1 {
                self.buffer.push_str("\x1b[A");
            } else {
                write!(self.buffer, "\x1b[{row_shift}A")?;
            }
        }
        let col_ordering = new.col.cmp(&old.col);
        if col_ordering == cmp::Ordering::Greater {
            // move right
            let col_shift = new.col - old.col;
            if col_shift == 1 {
                self.buffer.push_str("\x1b[C");
            } else {
                write!(self.buffer, "\x1b[{col_shift}C")?;
            }
        } else if col_ordering == cmp::Ordering::Less {
            // move left
            let col_shift = old.col - new.col;
            if col_shift == 1 {
                self.buffer.push_str("\x1b[D");
            } else {
                write!(self.buffer, "\x1b[{col_shift}D")?;
            }
        }
        write_all(self.out, self.buffer.as_str())?;
        Ok(())
    }

    fn refresh_line(
        &mut self,
        prompt: &str,
        line: &LineBuffer,
        old_layout: Option<&Layout>,
        new_layout: &Layout,
    ) -> Result<()> {
        use std::fmt::Write as _;
        self.begin_synchronized_update()?;
        self.buffer.clear();

        let cursor = new_layout.cursor;
        let end_pos = new_layout.end;

        if let Some(old_layout) = old_layout {
            self.clear_old_rows(old_layout);
        }

        // display the prompt
        self.buffer.push_str(prompt);
        // display the input line
        self.buffer.push_str(line);
        // we have to generate our own newline on line wrap
        if end_pos.col == 0 && end_pos.row > 0 && !line.ends_with('\n') {
            self.buffer.push('\n');
        }
        // position the cursor
        let new_cursor_row_movement = end_pos.row - cursor.row;
        // move the cursor up as required
        if new_cursor_row_movement > 0 {
            write!(self.buffer, "\x1b[{new_cursor_row_movement}A")?;
        }
        // position the cursor within the line
        if cursor.col > 0 {
            write!(self.buffer, "\r\x1b[{}C", cursor.col)?;
        } else {
            self.buffer.push('\r');
        }

        write_all(self.out, self.buffer.as_str())?;
        self.end_synchronized_update()?;
        Ok(())
    }

    fn write_and_flush(&mut self, buf: &str) -> Result<()> {
        write_all(self.out, buf)?;
        Ok(())
    }

    /// Control characters are treated as having zero width.
    /// Characters with 2 column width are correctly handled (not split).
    fn calculate_position(&self, s: &str, orig: Position) -> Position {
        let mut pos = orig;
        let mut esc_seq = 0;
        for c in graphemes(s) {
            if c == "\n" {
                pos.row += 1;
                pos.col = 0;
                continue;
            }
            let cw = if c == "\t" {
                self.tab_stop - (pos.col % self.tab_stop)
            } else {
                width(self.grapheme_cluster_mode, c, &mut esc_seq)
            };
            pos.col += cw;
            if pos.col > self.cols {
                pos.row += 1;
                pos.col = cw;
            }
        }
        if pos.col == self.cols {
            pos.col = 0;
            pos.row += 1;
        }
        pos
    }

    /// Clear the screen. Used to handle ctrl+l
    fn clear_screen(&mut self) -> Result<()> {
        self.write_and_flush("\x1b[H\x1b[J")
    }

    /// Clear from cursor to end of line. Used to optimize deletion at EOL
    fn clear_to_eol(&mut self) -> Result<()> {
        self.write_and_flush("\x1b[K")
    }

    /// Try to update the number of columns in the current terminal,
    fn update_size(&mut self) {
        let (cols, _) = get_win_size(self.out);
        self.cols = cols;
    }

    fn get_columns(&self) -> Unit {
        self.cols
    }

    fn grapheme_cluster_mode(&self) -> GraphemeClusterMode {
        self.grapheme_cluster_mode
    }
}

fn write_all(fd: RawFd, buf: &str) -> io::Result<()> {
    let mut bytes = buf.as_bytes();
    while !bytes.is_empty() {
        // SAFETY: bytes is valid for `bytes.len()` bytes
        let rc = unsafe { ffi::write(fd, bytes.as_ptr().cast::<c_void>(), bytes.len()) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        } else if rc == 0 {
            return Err(io::Error::from(ErrorKind::WriteZero));
        }
        #[allow(clippy::cast_sign_loss)]
        let n = rc as usize;
        bytes = &bytes[n..];
    }
    Ok(())
}

static SIG_PIPE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn sig_handler(sig: c_int) {
    let b: u8 = if sig == sys::SIGINT { b'I' } else { b'W' };
    let fd = SIG_PIPE.load(Ordering::Relaxed);
    if fd != -1 {
        // SAFETY: write(2) is async-signal-safe
        unsafe {
            ffi::write(fd, (&b as *const u8).cast::<c_void>(), 1);
        }
    }
}

struct Sig {
    pipe: UnixStream,
    original_sigint: ffi::SigAction,
    original_sigwinch: ffi::SigAction,
}

impl Sig {
    fn install_sigwinch_handler() -> Result<Self> {
        let (pipe, pipe_write) = UnixStream::pair()?;
        pipe.set_nonblocking(true)?;
        pipe_write.set_nonblocking(true)?;
        let pipe_write = pipe_write.into_raw_fd();
        if SIG_PIPE
            .compare_exchange(-1, pipe_write, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            // SAFETY: we own this file descriptor
            drop(unsafe { UnixStream::from_raw_fd(pipe_write) });
            return Err(
                io::Error::other("previous signal handler should have been uninstalled").into(),
            );
        }
        // empty mask, no flags
        let mut sa = ffi::SigAction([0; 256]);
        let handler = sig_handler as extern "C" fn(c_int) as usize;
        sa.0[sys::SA_HANDLER_OFFSET..sys::SA_HANDLER_OFFSET + std::mem::size_of::<usize>()]
            .copy_from_slice(&handler.to_ne_bytes());
        let mut original_sigint = ffi::SigAction([0; 256]);
        let mut original_sigwinch = ffi::SigAction([0; 256]);
        // SAFETY: installing a signal handler that only does a write(2)
        unsafe {
            if ffi::sigaction(sys::SIGINT, &sa, &mut original_sigint) != 0 {
                let err = io::Error::last_os_error();
                close_sig_pipe();
                return Err(err.into());
            }
            if ffi::sigaction(sys::SIGWINCH, &sa, &mut original_sigwinch) != 0 {
                let err = io::Error::last_os_error();
                ffi::sigaction(sys::SIGINT, &original_sigint, std::ptr::null_mut());
                close_sig_pipe();
                return Err(err.into());
            }
        }
        Ok(Self {
            pipe,
            original_sigint,
            original_sigwinch,
        })
    }

    fn uninstall_sigwinch_handler(self) {
        // SAFETY: restoring the original signal handlers
        unsafe {
            ffi::sigaction(sys::SIGINT, &self.original_sigint, std::ptr::null_mut());
            ffi::sigaction(sys::SIGWINCH, &self.original_sigwinch, std::ptr::null_mut());
        }
        close_sig_pipe();
    }
}

fn close_sig_pipe() {
    let fd = SIG_PIPE.swap(-1, Ordering::Relaxed);
    if fd != -1 {
        // SAFETY: we own this file descriptor
        drop(unsafe { UnixStream::from_raw_fd(fd) });
    }
}

impl Drop for PosixRawReader {
    fn drop(&mut self) {
        if let Some(sig) = self.sig.take() {
            sig.uninstall_sigwinch_handler();
        }
    }
}

#[cfg(not(test))]
pub type Terminal = PosixTerminal;

#[derive(Clone, Debug)]
pub struct PosixTerminal {
    unsupported: bool,
    tty_in: RawFd,
    is_in_a_tty: bool,
    tty_out: RawFd,
    is_out_a_tty: bool,
}

impl Term for PosixTerminal {
    type Buffer = PosixBuffer;
    type KeyMap = PosixKeyMap;
    type Mode = PosixMode;
    type Reader = PosixRawReader;
    type Writer = PosixRenderer;

    fn new(_config: &Config) -> Result<Self> {
        Ok(Self {
            unsupported: !sys::SUPPORTED || super::is_unsupported_term(),
            tty_in: io::stdin().as_raw_fd(),
            is_in_a_tty: io::stdin().is_terminal(),
            tty_out: io::stdout().as_raw_fd(),
            is_out_a_tty: io::stdout().is_terminal(),
        })
    }

    // Init checks:

    /// Check if current terminal can provide a rich line-editing user
    /// interface.
    fn is_unsupported(&self) -> bool {
        self.unsupported
    }

    fn is_input_tty(&self) -> bool {
        self.is_in_a_tty
    }

    // Interactive loop:

    fn enable_raw_mode(&mut self, c: &Config) -> Result<(Self::Mode, PosixKeyMap)> {
        if !self.is_in_a_tty {
            return Err(io::Error::from_raw_os_error(sys::ENOTTY).into());
        }
        let (original_mode, key_map) = termios_::enable_raw_mode(self.tty_in, c.enable_signals())?;

        // enable bracketed paste
        let out = if !c.enable_bracketed_paste()
            || write_all(self.tty_out, BRACKETED_PASTE_ON).is_err()
        {
            None
        } else {
            Some(self.tty_out)
        };

        Ok((
            PosixMode {
                termios: original_mode,
                tty_in: self.tty_in,
                tty_out: out,
            },
            key_map,
        ))
    }

    /// Create a RAW reader
    fn create_reader(
        &self,
        buffer: Option<PosixBuffer>,
        _config: &Config,
        key_map: PosixKeyMap,
    ) -> Result<PosixRawReader> {
        debug_assert!(!self.unsupported && self.is_in_a_tty);
        let sig = if self.is_out_a_tty {
            Some(Sig::install_sigwinch_handler()?)
        } else {
            None
        };
        Ok(PosixRawReader::new(self.tty_in, sig, buffer, key_map))
    }

    fn create_writer(&self, c: &Config) -> PosixRenderer {
        PosixRenderer::new(
            self.tty_out,
            Unit::from(c.tab_stop()),
            c.enable_synchronized_output(),
            GraphemeClusterMode::from_env(),
        )
    }

    fn writeln(&self) -> Result<()> {
        write_all(self.tty_out, "\n")?;
        Ok(())
    }
}

#[cfg(not(test))]
pub fn suspend() -> Result<()> {
    // suspend the whole process group
    // SAFETY: plain syscall
    if unsafe { ffi::kill(0, sys::SIGTSTP) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

mod termios_ {
    use std::collections::HashMap;
    use std::io;
    use std::os::unix::io::RawFd;

    use super::{ffi, sys, PosixKeyMap, Termios, CFLAG, IFLAG, LFLAG};
    use crate::keymap::Cmd;
    use crate::keys::{KeyEvent, Modifiers as M};
    use crate::Result;

    pub fn disable_raw_mode(tty_in: RawFd, termios: &Termios) -> Result<()> {
        // SAFETY: termios was filled by tcgetattr
        if unsafe { ffi::tcsetattr(tty_in, sys::TCSADRAIN, termios) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn enable_raw_mode(tty_in: RawFd, enable_signals: bool) -> Result<(Termios, PosixKeyMap)> {
        let mut original_mode = Termios([0; 256]);
        // SAFETY: the buffer is larger than any `struct termios`
        if unsafe { ffi::tcgetattr(tty_in, &mut original_mode) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        let mut raw = original_mode;

        // https://linux.die.net/man/3/termios
        // > The above symbolic subscript values are all different, except that
        // > VTIME,
        // > VMIN may have the same value as VEOL, VEOF, respectively.
        // > In noncanonical mode the special character meaning is replaced by
        // > the
        // > timeout meaning.
        // So we must read VEOF before writing VTIME
        let mut key_map: HashMap<KeyEvent, Cmd> = HashMap::with_capacity(4);
        map_key(&mut key_map, &raw, sys::VEOF, Cmd::EndOfFile);
        map_key(&mut key_map, &raw, sys::VINTR, Cmd::Interrupt);
        map_key(&mut key_map, &raw, sys::VQUIT, Cmd::Interrupt);
        map_key(&mut key_map, &raw, sys::VSUSP, Cmd::Suspend);

        // disable BREAK interrupt, CR to NL conversion on input,
        // input parity check, strip high bit (bit 8), output flow control
        raw.set_flag(
            IFLAG,
            raw.flag(IFLAG) & !(sys::BRKINT | sys::ICRNL | sys::INPCK | sys::ISTRIP | sys::IXON),
        );
        // we don't want raw output, it turns newlines into straight line feeds
        // disable all output processing
        // raw.c_oflag = raw.c_oflag & !(OutputFlags::OPOST);

        // character-size mark (8 bits)
        raw.set_flag(CFLAG, raw.flag(CFLAG) | sys::CS8);
        // disable echoing, canonical mode, extended input processing and
        // signals
        let mut lflag = raw.flag(LFLAG) & !(sys::ECHO | sys::ICANON | sys::IEXTEN | sys::ISIG);
        if enable_signals {
            lflag |= sys::ISIG;
        }
        raw.set_flag(LFLAG, lflag);

        raw.set_cc(sys::VMIN, 1); // One character-at-a-time input
        raw.set_cc(sys::VTIME, 0); // with blocking read

        // SAFETY: raw is a modified copy of what tcgetattr returned
        if unsafe { ffi::tcsetattr(tty_in, sys::TCSADRAIN, &raw) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok((original_mode, key_map))
    }

    fn map_key(key_map: &mut HashMap<KeyEvent, Cmd>, raw: &Termios, index: usize, cmd: Cmd) {
        let cc = char::from(raw.cc(index));
        let key = KeyEvent::new(cc, M::NONE);
        key_map.insert(key, cmd);
    }
}

#[cfg(test)]
mod test {
    use super::{Position, PosixRawReader, PosixRenderer, PosixTerminal, Renderer as _};
    use crate::keys::{KeyCode as K, KeyEvent as E, Modifiers as M};
    use crate::layout::GraphemeClusterMode;
    use crate::line_buffer::{LineBuffer, NoListener};
    use crate::tty::RawReader as _;

    #[test]
    #[ignore]
    fn prompt_with_ansi_escape_codes() {
        let out = PosixRenderer::new(1, 4, true, GraphemeClusterMode::default());
        let pos = out.calculate_position("\x1b[1;32m>>\x1b[0m ", Position::default());
        assert_eq!(3, pos.col);
        assert_eq!(0, pos.row);
    }

    #[test]
    fn test_send() {
        fn assert_send<T: Send>() {}
        assert_send::<PosixTerminal>();
    }

    #[test]
    fn test_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<PosixTerminal>();
    }

    #[test]
    fn test_line_wrap() {
        let mut out = PosixRenderer::new(1, 4, true, GraphemeClusterMode::default());
        // don't write to the actual stdout
        out.out = -1;
        let prompt = "> ";
        let default_prompt = true;
        let prompt_size = out.calculate_position(prompt, Position::default());

        let mut line = LineBuffer::init("", 0);
        let old_layout = out.compute_layout(prompt_size, default_prompt, &line);
        assert_eq!(Position { col: 2, row: 0 }, old_layout.cursor);
        assert_eq!(old_layout.cursor, old_layout.end);

        assert!(line.insert('a', out.cols - prompt_size.col + 1, &mut NoListener));
        let new_layout = out.compute_layout(prompt_size, default_prompt, &line);
        assert_eq!(Position { col: 1, row: 1 }, new_layout.cursor);
        assert_eq!(new_layout.cursor, new_layout.end);
        // writing to fd -1 fails, but the buffer is filled anyway
        out.enable_synchronized_output = false;
        let _ = out.refresh_line(prompt, &line, Some(&old_layout), &new_layout);
        #[rustfmt::skip]
        assert_eq!(
            "\r\u{1b}[K> aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\u{1b}[1C",
            out.buffer
        );
    }

    #[test]
    fn test_sigwinch_handler() {
        use super::{ffi, sys, Sig};
        use crate::error::Signal;
        use crate::ReadlineError;

        let sig = Sig::install_sigwinch_handler().unwrap();
        // SAFETY: SIGWINCH is harmless (ignored by default)
        assert_eq!(0, unsafe {
            ffi::kill(std::process::id() as i32, sys::SIGWINCH)
        });
        let mut rdr = PosixRawReader::new(-1, Some(sig), None, Default::default());
        assert!(matches!(
            rdr.next_char(),
            Err(ReadlineError::Signal(Signal::Resize))
        ));
        drop(rdr);

        // the original (default) disposition is restored
        let mut current = ffi::SigAction([0; 256]);
        // SAFETY: querying the current action only
        assert_eq!(0, unsafe {
            ffi::sigaction(sys::SIGWINCH, std::ptr::null(), &mut current)
        });
        let off = sys::SA_HANDLER_OFFSET;
        assert!(current.0[off..off + std::mem::size_of::<usize>()]
            .iter()
            .all(|&b| b == 0));
    }

    fn reader(input: &[u8]) -> PosixRawReader {
        PosixRawReader::new(-1, None, Some(input.to_vec()), Default::default())
    }

    #[test]
    fn test_utf8_decoding() {
        let mut rdr = reader("aß€😀".as_bytes());
        assert_eq!('a', rdr.next_char().unwrap());
        assert_eq!('ß', rdr.next_char().unwrap());
        assert_eq!('€', rdr.next_char().unwrap());
        assert_eq!('😀', rdr.next_char().unwrap());
        let mut rdr = reader(b"\xff");
        assert!(rdr.next_char().is_err());
        let mut rdr = reader(b"\xe0\x80\x80");
        assert!(rdr.next_char().is_err());
    }

    #[test]
    fn test_escape_sequences() {
        let cases: &[(&[u8], E)] = &[
            (b"\x1b[A", E(K::Up, M::NONE)),
            (b"\x1b[1;5C", E(K::Right, M::CTRL)),
            (b"\x1b[1;3D", E(K::Left, M::ALT)),
            (b"\x1b[1;9A", E(K::Up, M::ALT)),
            (b"\x1b[1;5p", E(K::Char('0'), M::CTRL)),
            (b"\x1b[1;6y", E(K::Char('9'), M::CTRL_SHIFT)),
            (b"\x1b[1;2P", E(K::UnknownEscSeq, M::NONE)),
            (b"\x1b[1;5P", E(K::F(1), M::CTRL)),
            (b"\x1b[3~", E(K::Delete, M::NONE)),
            (b"\x1b[3;5~", E(K::Delete, M::CTRL)),
            (b"\x1b[5;2~", E(K::PageUp, M::SHIFT)),
            (b"\x1b[200~", E(K::BracketedPasteStart, M::NONE)),
            (b"\x1bOH", E(K::Home, M::NONE)),
            (b"\x1bb", E(K::Char('b'), M::ALT)),
            (b"\x1b\x1b[D", E(K::Left, M::ALT)),
            (b"\x7f", E(K::Backspace, M::NONE)),
            (b"\r", E(K::Enter, M::NONE)),
        ];
        for (input, expected) in cases {
            let mut rdr = reader(input);
            assert_eq!(*expected, rdr.next_key(false).unwrap(), "{input:?}");
            assert!(rdr.unbuffer().is_none(), "{input:?}");
        }
    }

    #[test]
    fn test_bracketed_paste() {
        let mut rdr = reader(b"\x1b[200~foo\r\nbar\x1b[201~rest");
        assert_eq!(
            E(K::BracketedPasteStart, M::NONE),
            rdr.next_key(false).unwrap()
        );
        assert_eq!("foo\nbar", rdr.read_pasted_text().unwrap());
        assert_eq!(Some(b"rest".to_vec()), rdr.unbuffer());
    }
}
