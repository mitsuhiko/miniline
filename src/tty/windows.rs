//! Windows specific definitions
//!
//! This talks to the Win32 console API directly (no `windows-sys` crate).
use std::{io, mem, ptr};

use super::{width, RawMode, RawReader, Renderer, Term};
use crate::config::Config;
use crate::error;
use crate::keymap::Cmd;
use crate::keys::{KeyCode as K, KeyEvent, Modifiers as M};
use crate::layout::{GraphemeClusterMode, Layout, Position, Unit};
use crate::line_buffer::LineBuffer;
use crate::unicode::graphemes;
use crate::Result;

#[allow(
    non_snake_case,
    non_camel_case_types,
    clippy::upper_case_acronyms,
    dead_code
)]
mod ffi {
    use std::os::raw::c_void;

    pub type HANDLE = *mut c_void;
    pub type BOOL = i32;
    pub type DWORD = u32;
    pub type WORD = u16;

    pub const FALSE: BOOL = 0;
    pub const TRUE: BOOL = 1;
    pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
    pub const STD_INPUT_HANDLE: DWORD = -10i32 as DWORD;
    pub const STD_OUTPUT_HANDLE: DWORD = -11i32 as DWORD;
    pub const ERROR_INVALID_PARAMETER: i32 = 87;

    // console input modes
    pub const ENABLE_PROCESSED_INPUT: DWORD = 0x0001;
    pub const ENABLE_LINE_INPUT: DWORD = 0x0002;
    pub const ENABLE_ECHO_INPUT: DWORD = 0x0004;
    pub const ENABLE_WINDOW_INPUT: DWORD = 0x0008;
    pub const ENABLE_INSERT_MODE: DWORD = 0x0020;
    pub const ENABLE_QUICK_EDIT_MODE: DWORD = 0x0040;
    pub const ENABLE_EXTENDED_FLAGS: DWORD = 0x0080;
    // console output modes
    pub const ENABLE_WRAP_AT_EOL_OUTPUT: DWORD = 0x0002;
    pub const ENABLE_VIRTUAL_TERMINAL_PROCESSING: DWORD = 0x0004;

    // input record event types
    pub const KEY_EVENT: WORD = 0x0001;
    pub const WINDOW_BUFFER_SIZE_EVENT: WORD = 0x0004;

    // control key state
    pub const RIGHT_ALT_PRESSED: DWORD = 0x0001;
    pub const LEFT_ALT_PRESSED: DWORD = 0x0002;
    pub const RIGHT_CTRL_PRESSED: DWORD = 0x0004;
    pub const LEFT_CTRL_PRESSED: DWORD = 0x0008;
    pub const SHIFT_PRESSED: DWORD = 0x0010;

    // virtual key codes
    pub const VK_BACK: WORD = 0x08;
    pub const VK_TAB: WORD = 0x09;
    pub const VK_RETURN: WORD = 0x0D;
    pub const VK_MENU: WORD = 0x12;
    pub const VK_ESCAPE: WORD = 0x1B;
    pub const VK_PRIOR: WORD = 0x21;
    pub const VK_NEXT: WORD = 0x22;
    pub const VK_END: WORD = 0x23;
    pub const VK_HOME: WORD = 0x24;
    pub const VK_LEFT: WORD = 0x25;
    pub const VK_UP: WORD = 0x26;
    pub const VK_RIGHT: WORD = 0x27;
    pub const VK_DOWN: WORD = 0x28;
    pub const VK_INSERT: WORD = 0x2D;
    pub const VK_DELETE: WORD = 0x2E;
    pub const VK_F1: WORD = 0x70;
    pub const VK_F12: WORD = 0x7B;

    pub const CF_UNICODETEXT: u32 = 13;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct COORD {
        pub X: i16,
        pub Y: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SMALL_RECT {
        pub Left: i16,
        pub Top: i16,
        pub Right: i16,
        pub Bottom: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CONSOLE_SCREEN_BUFFER_INFO {
        pub dwSize: COORD,
        pub dwCursorPosition: COORD,
        pub wAttributes: WORD,
        pub srWindow: SMALL_RECT,
        pub dwMaximumWindowSize: COORD,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CONSOLE_CURSOR_INFO {
        pub dwSize: DWORD,
        pub bVisible: BOOL,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct KEY_EVENT_RECORD {
        pub bKeyDown: BOOL,
        pub wRepeatCount: WORD,
        pub wVirtualKeyCode: WORD,
        pub wVirtualScanCode: WORD,
        pub UnicodeChar: u16,
        pub dwControlKeyState: DWORD,
    }

    /// `INPUT_RECORD` where the event union is represented by its largest
    /// (16 byte) member, the key event record.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct INPUT_RECORD {
        pub EventType: WORD,
        pub KeyEvent: KEY_EVENT_RECORD,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetStdHandle(nStdHandle: DWORD) -> HANDLE;
        pub fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *mut DWORD) -> BOOL;
        pub fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: DWORD) -> BOOL;
        pub fn ReadConsoleInputW(
            hConsoleInput: HANDLE,
            lpBuffer: *mut INPUT_RECORD,
            nLength: DWORD,
            lpNumberOfEventsRead: *mut DWORD,
        ) -> BOOL;
        pub fn GetConsoleScreenBufferInfo(
            hConsoleOutput: HANDLE,
            lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
        ) -> BOOL;
        pub fn SetConsoleCursorPosition(hConsoleOutput: HANDLE, dwCursorPosition: COORD) -> BOOL;
        pub fn FillConsoleOutputCharacterW(
            hConsoleOutput: HANDLE,
            cCharacter: u16,
            nLength: DWORD,
            dwWriteCoord: COORD,
            lpNumberOfCharsWritten: *mut DWORD,
        ) -> BOOL;
        pub fn FillConsoleOutputAttribute(
            hConsoleOutput: HANDLE,
            wAttribute: WORD,
            nLength: DWORD,
            dwWriteCoord: COORD,
            lpNumberOfAttrsWritten: *mut DWORD,
        ) -> BOOL;
        pub fn GetConsoleCursorInfo(
            hConsoleOutput: HANDLE,
            lpConsoleCursorInfo: *mut CONSOLE_CURSOR_INFO,
        ) -> BOOL;
        pub fn SetConsoleCursorInfo(
            hConsoleOutput: HANDLE,
            lpConsoleCursorInfo: *const CONSOLE_CURSOR_INFO,
        ) -> BOOL;
        pub fn WriteConsoleW(
            hConsoleOutput: HANDLE,
            lpBuffer: *const u16,
            nNumberOfCharsToWrite: DWORD,
            lpNumberOfCharsWritten: *mut DWORD,
            lpReserved: *mut c_void,
        ) -> BOOL;
        pub fn GlobalLock(hMem: HANDLE) -> *mut c_void;
        pub fn GlobalUnlock(hMem: HANDLE) -> BOOL;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn OpenClipboard(hWndNewOwner: HANDLE) -> BOOL;
        pub fn CloseClipboard() -> BOOL;
        pub fn GetClipboardData(uFormat: u32) -> HANDLE;
    }
}

use self::ffi::{BOOL, COORD, DWORD, FALSE, HANDLE, TRUE};

fn get_std_handle(fd: DWORD) -> Result<HANDLE> {
    let handle = unsafe { ffi::GetStdHandle(fd) };
    check_handle(handle)
}

fn check_handle(handle: HANDLE) -> Result<HANDLE> {
    if handle == ffi::INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())?;
    } else if handle.is_null() {
        Err(io::Error::other(
            "no stdio handle available for this process",
        ))?;
    }
    Ok(handle)
}

fn check(rc: BOOL) -> io::Result<()> {
    if rc == FALSE {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn get_win_size(handle: HANDLE) -> (Unit, Unit) {
    let mut info = ffi::CONSOLE_SCREEN_BUFFER_INFO::default();
    match unsafe { ffi::GetConsoleScreenBufferInfo(handle, &mut info) } {
        FALSE => (80, 24),
        _ => (
            Unit::try_from(info.dwSize.X).unwrap_or(80),
            Unit::try_from(1 + info.srWindow.Bottom - info.srWindow.Top).unwrap_or(24),
        ), // (info.srWindow.Right - info.srWindow.Left + 1)
    }
}

fn get_console_mode(handle: HANDLE) -> Result<DWORD> {
    let mut original_mode = 0;
    check(unsafe { ffi::GetConsoleMode(handle, &mut original_mode) })?;
    Ok(original_mode)
}

/// Read unicode text from the clipboard
fn get_clipboard_string() -> Result<String> {
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe { ffi::CloseClipboard() };
        }
    }

    check(unsafe { ffi::OpenClipboard(ptr::null_mut()) })?;
    let _clipboard = Clipboard;
    let data = unsafe { ffi::GetClipboardData(ffi::CF_UNICODETEXT) };
    if data.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    let text = unsafe { ffi::GlobalLock(data) }.cast::<u16>();
    if text.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    let mut len = 0;
    // SAFETY: CF_UNICODETEXT data is a null terminated wide string
    let rv = unsafe {
        while *text.add(len) != 0 {
            len += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(text, len))
    };
    unsafe { ffi::GlobalUnlock(data) };
    rv.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e).into())
}

type ConsoleBuffer = ();

type ConsoleKeyMap = ();
#[cfg(not(test))]
pub type KeyMap = ConsoleKeyMap;

#[cfg(not(test))]
pub type Mode = ConsoleMode;

#[must_use = "You must restore default mode (disable_raw_mode)"]
#[derive(Clone, Debug)]
pub struct ConsoleMode {
    original_conin_mode: DWORD,
    conin: HANDLE,
    original_conout_mode: Option<DWORD>,
    conout: HANDLE,
}

impl RawMode for ConsoleMode {
    /// Disable RAW mode for the terminal.
    fn disable_raw_mode(&self) -> Result<()> {
        check(unsafe { ffi::SetConsoleMode(self.conin, self.original_conin_mode) })?;
        if let Some(original_stdstream_mode) = self.original_conout_mode {
            check(unsafe { ffi::SetConsoleMode(self.conout, original_stdstream_mode) })?;
        }
        Ok(())
    }
}

/// Console input reader
pub struct ConsoleRawReader {
    conin: HANDLE,
}

impl ConsoleRawReader {
    fn create(conin: HANDLE) -> Self {
        Self { conin }
    }
}

impl RawReader for ConsoleRawReader {
    type Buffer = ConsoleBuffer;

    fn next_key(&mut self, _: bool) -> Result<KeyEvent> {
        read_input(self.conin, u32::MAX)
    }

    fn read_pasted_text(&mut self) -> Result<String> {
        get_clipboard_string()
    }

    fn find_binding(&self, _: &KeyEvent) -> Option<Cmd> {
        None
    }

    fn unbuffer(self) -> Option<ConsoleBuffer> {
        None
    }
}

fn read_input(handle: HANDLE, max_count: u32) -> Result<KeyEvent> {
    use std::char::decode_utf16;

    use self::ffi::{
        LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED, SHIFT_PRESSED,
    };

    let mut rec = ffi::INPUT_RECORD::default();
    let mut count = 0;
    let mut total = 0;
    let mut surrogate = 0;
    loop {
        if total >= max_count {
            return Ok(KeyEvent(K::UnknownEscSeq, M::NONE));
        }
        check(unsafe { ffi::ReadConsoleInputW(handle, &mut rec, 1, &mut count) })?;
        total += count;

        if rec.EventType == ffi::WINDOW_BUFFER_SIZE_EVENT {
            return Err(error::ReadlineError::Signal(error::Signal::Resize));
        } else if rec.EventType != ffi::KEY_EVENT {
            continue;
        }
        let key_event = rec.KeyEvent;
        if key_event.bKeyDown == 0 && key_event.wVirtualKeyCode != ffi::VK_MENU {
            continue;
        }
        // key_event.wRepeatCount seems to be always set to 1 (maybe because we
        // only read one character at a time)

        let alt_gr = key_event.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED)
            == (LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED);
        let mut mods = M::NONE;
        if !alt_gr && key_event.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
            mods |= M::CTRL;
        }
        if !alt_gr && key_event.dwControlKeyState & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
            mods |= M::ALT;
        }
        if key_event.dwControlKeyState & SHIFT_PRESSED != 0 {
            mods |= M::SHIFT;
        }

        let utf16 = key_event.UnicodeChar;
        let key_code = match key_event.wVirtualKeyCode {
            ffi::VK_LEFT => K::Left,
            ffi::VK_RIGHT => K::Right,
            ffi::VK_UP => K::Up,
            ffi::VK_DOWN => K::Down,
            ffi::VK_DELETE => K::Delete,
            ffi::VK_HOME => K::Home,
            ffi::VK_END => K::End,
            ffi::VK_PRIOR => K::PageUp,
            ffi::VK_NEXT => K::PageDown,
            ffi::VK_INSERT => K::Insert,
            vk @ ffi::VK_F1..=ffi::VK_F12 => K::F((vk - ffi::VK_F1 + 1) as u8),
            ffi::VK_BACK => K::Backspace, // vs Ctrl-h
            ffi::VK_RETURN => K::Enter,   // vs Ctrl-m
            ffi::VK_ESCAPE => K::Esc,
            ffi::VK_TAB => {
                if mods.contains(M::SHIFT) {
                    mods.remove(M::SHIFT);
                    K::BackTab
                } else {
                    K::Tab // vs Ctrl-i
                }
            }
            _ => {
                if utf16 == 0 {
                    continue;
                }
                K::UnknownEscSeq
            }
        };

        let key = if key_code != K::UnknownEscSeq {
            KeyEvent(key_code, mods)
        } else if utf16 == 27 {
            KeyEvent(K::Esc, mods) // FIXME dead code ?
        } else {
            if (0xD800..0xDC00).contains(&utf16) {
                surrogate = utf16;
                continue;
            }
            let orc = if surrogate == 0 {
                decode_utf16(Some(utf16)).next()
            } else {
                decode_utf16([surrogate, utf16].iter().copied()).next()
            };
            let Some(rc) = orc else {
                return Err(error::ReadlineError::Eof);
            };
            let c = rc?;
            KeyEvent::new(c, mods)
        };
        return Ok(key);
    }
}

pub struct ConsoleRenderer {
    conout: HANDLE,
    cols: Unit, // Number of columns in terminal
    buffer: String,
    utf16: Vec<u16>,
    colors_enabled: bool,
    grapheme_cluster_mode: GraphemeClusterMode,
}

impl ConsoleRenderer {
    fn new(
        conout: HANDLE,
        colors_enabled: bool,
        grapheme_cluster_mode: GraphemeClusterMode,
    ) -> Self {
        // Multi line editing is enabled by ENABLE_WRAP_AT_EOL_OUTPUT mode
        let (cols, _) = get_win_size(conout);
        Self {
            conout,
            cols,
            buffer: String::with_capacity(1024),
            utf16: Vec::with_capacity(1024),
            colors_enabled,
            grapheme_cluster_mode,
        }
    }

    fn get_console_screen_buffer_info(&self) -> Result<ffi::CONSOLE_SCREEN_BUFFER_INFO> {
        let mut info = ffi::CONSOLE_SCREEN_BUFFER_INFO::default();
        check(unsafe { ffi::GetConsoleScreenBufferInfo(self.conout, &mut info) })?;
        Ok(info)
    }

    fn set_console_cursor_position(&mut self, mut pos: COORD, size: COORD) -> Result<COORD> {
        use std::cmp::{max, min};
        // https://docs.microsoft.com/en-us/windows/console/setconsolecursorposition
        // > The coordinates must be within the boundaries of the console screen
        // > buffer.
        // pos.X = max(0, min(size.X - 1, pos.X));
        pos.Y = max(0, min(size.Y - 1, pos.Y));
        check(unsafe { ffi::SetConsoleCursorPosition(self.conout, pos) })?;
        Ok(pos)
    }

    fn clear(&mut self, length: u32, pos: COORD, attr: u16) -> Result<()> {
        let mut _count = 0;
        check(unsafe {
            ffi::FillConsoleOutputCharacterW(self.conout, u16::from(b' '), length, pos, &mut _count)
        })?;
        Ok(check(unsafe {
            ffi::FillConsoleOutputAttribute(self.conout, attr, length, pos, &mut _count)
        })?)
    }

    fn set_cursor_visibility(&mut self, visible: bool) -> Result<Option<ConsoleCursorGuard>> {
        set_cursor_visibility(self.conout, visible)
    }

    // You can't have both ENABLE_WRAP_AT_EOL_OUTPUT and
    // ENABLE_VIRTUAL_TERMINAL_PROCESSING. So we need to wrap manually.
    fn wrap_at_eol(&mut self, s: &str, mut col: Unit) -> Unit {
        let mut esc_seq = 0;
        for c in graphemes(s) {
            if c == "\n" {
                col = 0;
            } else {
                let cw = width(self.grapheme_cluster_mode, c, &mut esc_seq);
                col += cw;
                if col > self.cols {
                    self.buffer.push('\n');
                    col = cw;
                }
            }
            self.buffer.push_str(c);
        }
        if col == self.cols {
            self.buffer.push('\n');
            col = 0;
        }
        col
    }

    // position at the start of the prompt, clear to end of previous input
    fn clear_old_rows(
        &mut self,
        info: &ffi::CONSOLE_SCREEN_BUFFER_INFO,
        layout: &Layout,
    ) -> Result<()> {
        let current_row = layout.cursor.row;
        let old_rows = layout.end.row;
        let mut coord = info.dwCursorPosition;
        coord.X = 0;
        coord.Y -= current_row as i16;
        let coord = self.set_console_cursor_position(coord, info.dwSize)?;
        self.clear(
            (info.dwSize.X as u32) * (u32::from(old_rows) + 1),
            coord,
            info.wAttributes,
        )
    }
}

pub struct ConsoleCursorGuard(HANDLE);

impl Drop for ConsoleCursorGuard {
    fn drop(&mut self) {
        let _ = set_cursor_visibility(self.0, true);
    }
}

fn set_cursor_visibility(handle: HANDLE, visible: bool) -> Result<Option<ConsoleCursorGuard>> {
    let mut info = ffi::CONSOLE_CURSOR_INFO::default();
    check(unsafe { ffi::GetConsoleCursorInfo(handle, &mut info) })?;
    let b = if visible { TRUE } else { FALSE };
    if info.bVisible == b {
        return Ok(None);
    }
    info.bVisible = b;
    check(unsafe { ffi::SetConsoleCursorInfo(handle, &info) })?;
    Ok(if visible {
        None
    } else {
        Some(ConsoleCursorGuard(handle))
    })
}

impl Renderer for ConsoleRenderer {
    type Reader = ConsoleRawReader;

    fn move_cursor(&mut self, old: Position, new: Position) -> Result<()> {
        let info = self.get_console_screen_buffer_info()?;
        let mut cursor = info.dwCursorPosition;
        if new.row > old.row {
            cursor.Y += (new.row - old.row) as i16;
        } else {
            cursor.Y -= (old.row - new.row) as i16;
        }
        if new.col > old.col {
            cursor.X += (new.col - old.col) as i16;
        } else {
            cursor.X -= (old.col - new.col) as i16;
        }
        self.set_console_cursor_position(cursor, info.dwSize)
            .map(|_| ())
    }

    fn refresh_line(
        &mut self,
        prompt: &str,
        line: &LineBuffer,
        old_layout: Option<&Layout>,
        new_layout: &Layout,
    ) -> Result<()> {
        let cursor = new_layout.cursor;
        let end_pos = new_layout.end;

        self.buffer.clear();
        if self.colors_enabled {
            let mut col = 0;
            // append the prompt
            col = self.wrap_at_eol(prompt, col);
            // append the input line
            self.wrap_at_eol(line, col);
        } else {
            // append the prompt
            self.buffer.push_str(prompt);
            // append the input line
            self.buffer.push_str(line);
        }
        let info = self.get_console_screen_buffer_info()?;
        // just to avoid flickering
        let mut guard = self.set_cursor_visibility(false)?;
        // position at the start of the prompt, clear to end of previous input
        if let Some(old_layout) = old_layout {
            self.clear_old_rows(&info, old_layout)?;
        }
        // display prompt, input line and hint
        write_to_console(self.conout, self.buffer.as_str(), &mut self.utf16)?;

        // position the cursor
        let info = self.get_console_screen_buffer_info()?;
        let mut coord = info.dwCursorPosition;
        coord.X = cursor.col as i16;
        coord.Y -= (end_pos.row - cursor.row) as i16;
        self.set_console_cursor_position(coord, info.dwSize)?;
        guard.take();
        Ok(())
    }

    fn write_and_flush(&mut self, buf: &str) -> Result<()> {
        write_to_console(self.conout, buf, &mut self.utf16)
    }

    /// Characters with 2 column width are correctly handled (not split).
    fn calculate_position(&self, s: &str, orig: Position) -> Position {
        let mut pos = orig;
        for c in graphemes(s) {
            if c == "\n" {
                pos.col = 0;
                pos.row += 1;
            } else {
                let cw = self.grapheme_cluster_mode.width(c);
                pos.col += cw;
                if pos.col > self.cols {
                    pos.row += 1;
                    pos.col = cw;
                }
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
        let info = self.get_console_screen_buffer_info()?;
        let coord = COORD { X: 0, Y: 0 };
        check(unsafe { ffi::SetConsoleCursorPosition(self.conout, coord) })?;
        let n = info.dwSize.X as u32 * info.dwSize.Y as u32;
        self.clear(n, coord, info.wAttributes)
    }

    /// Clear from cursor to end of line. Used to optimize deletion at EOL
    fn clear_to_eol(&mut self) -> Result<()> {
        let info = self.get_console_screen_buffer_info()?;
        let cursor = info.dwCursorPosition;
        let n = (info.dwSize.X - cursor.X) as u32;
        self.clear(n, cursor, info.wAttributes)
    }

    /// Try to get the number of columns in the current terminal,
    /// or assume 80 if it fails.
    fn update_size(&mut self) {
        let (cols, _) = get_win_size(self.conout);
        self.cols = cols;
    }

    fn get_columns(&self) -> Unit {
        self.cols
    }

    fn grapheme_cluster_mode(&self) -> GraphemeClusterMode {
        self.grapheme_cluster_mode
    }
}

fn write_to_console(handle: HANDLE, s: &str, utf16: &mut Vec<u16>) -> Result<()> {
    utf16.clear();
    utf16.extend(s.encode_utf16());
    write_all(handle, utf16.as_slice())
}

// See write_valid_utf8_to_console
// /src/rust/library/std/src/sys/windows/stdio.rs:171
fn write_all(handle: HANDLE, mut data: &[u16]) -> Result<()> {
    use std::io::{Error, ErrorKind};
    while !data.is_empty() {
        let slice = if data.len() < 8192 {
            data
        } else if (0xD800..0xDC00).contains(&data[8191]) {
            &data[..8191]
        } else {
            &data[..8192]
        };
        let mut written = 0;
        check(unsafe {
            ffi::WriteConsoleW(
                handle,
                slice.as_ptr(),
                slice.len() as u32,
                &mut written,
                ptr::null_mut(),
            )
        })?;
        if written == 0 {
            Err(Error::new(ErrorKind::WriteZero, "WriteConsoleW"))?;
        }
        data = &data[(written as usize)..];
    }
    Ok(())
}

#[cfg(not(test))]
pub type Terminal = Console;

#[derive(Clone, Debug)]
pub struct Console {
    conin_isatty: bool,
    conin: HANDLE,
    conout_isatty: bool,
    conout: HANDLE,
    ansi_colors_supported: bool,
}

impl Console {
    fn colors_enabled(&self) -> bool {
        self.conout_isatty && self.ansi_colors_supported
    }
}

/// Mirror of rustyline's default color mode (enabled unless `NO_COLOR` is
/// set).  It only affects whether virtual terminal processing is enabled.
fn color_mode_disabled() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|os| !os.is_empty())
}

impl Term for Console {
    type Buffer = ConsoleBuffer;
    type KeyMap = ConsoleKeyMap;
    type Mode = ConsoleMode;
    type Reader = ConsoleRawReader;
    type Writer = ConsoleRenderer;

    fn new(_config: &Config) -> Result<Self> {
        let conin = get_std_handle(ffi::STD_INPUT_HANDLE);
        let conout = get_std_handle(ffi::STD_OUTPUT_HANDLE);
        let conin_isatty = match conin {
            Ok(handle) => {
                // If this function doesn't fail then fd is a TTY
                get_console_mode(handle).is_ok()
            }
            Err(_) => false,
        };

        let conout_isatty = match conout {
            Ok(handle) => {
                // If this function doesn't fail then fd is a TTY
                get_console_mode(handle).is_ok()
            }
            Err(_) => false,
        };

        Ok(Self {
            conin_isatty,
            conin: conin.unwrap_or(ptr::null_mut()),
            conout_isatty,
            conout: conout.unwrap_or(ptr::null_mut()),
            ansi_colors_supported: false,
        })
    }

    fn is_unsupported(&self) -> bool {
        super::is_unsupported_term()
    }

    fn is_input_tty(&self) -> bool {
        self.conin_isatty
    }

    /// Enable RAW mode for the terminal.
    fn enable_raw_mode(&mut self, _config: &Config) -> Result<(ConsoleMode, ConsoleKeyMap)> {
        if !self.conin_isatty {
            Err(io::Error::other(
                "no stdio handle available for this process",
            ))?;
        }
        let original_conin_mode = get_console_mode(self.conin)?;
        // Disable these modes
        let mut raw = original_conin_mode
            & !(ffi::ENABLE_LINE_INPUT | ffi::ENABLE_ECHO_INPUT | ffi::ENABLE_PROCESSED_INPUT);
        // Enable these modes
        raw |= ffi::ENABLE_EXTENDED_FLAGS;
        raw |= ffi::ENABLE_INSERT_MODE;
        raw |= ffi::ENABLE_QUICK_EDIT_MODE;
        raw |= ffi::ENABLE_WINDOW_INPUT;
        check(unsafe { ffi::SetConsoleMode(self.conin, raw) })?;

        let original_conout_mode = if self.conout_isatty {
            let original_conout_mode = get_console_mode(self.conout)?;

            let mut mode = original_conout_mode;
            if mode & ffi::ENABLE_WRAP_AT_EOL_OUTPUT == 0 {
                mode |= ffi::ENABLE_WRAP_AT_EOL_OUTPUT;
                check(unsafe { ffi::SetConsoleMode(self.conout, mode) })?;
            }
            // To enable ANSI colors (Windows 10 only):
            // https://docs.microsoft.com/en-us/windows/console/setconsolemode
            self.ansi_colors_supported = mode & ffi::ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0;
            if self.ansi_colors_supported {
                if color_mode_disabled() {
                    mode &= !ffi::ENABLE_VIRTUAL_TERMINAL_PROCESSING;
                    check(unsafe { ffi::SetConsoleMode(self.conout, mode) })?;
                }
            } else if !color_mode_disabled() {
                mode |= ffi::ENABLE_VIRTUAL_TERMINAL_PROCESSING;
                self.ansi_colors_supported = unsafe { ffi::SetConsoleMode(self.conout, mode) != 0 };
            }
            Some(original_conout_mode)
        } else {
            None
        };

        Ok((
            ConsoleMode {
                original_conin_mode,
                conin: self.conin,
                original_conout_mode,
                conout: self.conout,
            },
            (),
        ))
    }

    fn create_reader(
        &self,
        _: Option<ConsoleBuffer>,
        _: &Config,
        _: ConsoleKeyMap,
    ) -> Result<ConsoleRawReader> {
        Ok(ConsoleRawReader::create(self.conin))
    }

    fn create_writer(&self, _config: &Config) -> ConsoleRenderer {
        ConsoleRenderer::new(
            self.conout,
            self.colors_enabled(),
            GraphemeClusterMode::from_env(),
        )
    }

    fn writeln(&self) -> Result<()> {
        write_all(self.conout, &[10; 1])
    }
}

// SAFETY: console handles can be used from any thread
unsafe impl Send for Console {}
unsafe impl Sync for Console {}

#[allow(dead_code)]
const _: () = {
    // sanity checks for the hand written FFI structures
    assert!(mem::size_of::<ffi::INPUT_RECORD>() == 20);
    assert!(mem::size_of::<ffi::KEY_EVENT_RECORD>() == 16);
    assert!(mem::size_of::<ffi::CONSOLE_SCREEN_BUFFER_INFO>() == 22);
    assert!(mem::size_of::<ffi::CONSOLE_CURSOR_INFO>() == 8);
};

#[cfg(test)]
mod test {
    use super::Console;

    #[test]
    fn test_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Console>();
    }

    #[test]
    fn test_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<Console>();
    }
}
