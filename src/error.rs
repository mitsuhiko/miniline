//! Contains error type for handling I/O errors
use std::error::Error;
use std::{fmt, io};

/// The error type for miniline errors that can arise from
/// I/O related errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadlineError {
    /// I/O Error
    Io(io::Error),
    /// EOF (VEOF / Ctrl-D)
    Eof,
    /// Interrupt signal (VINTR / VQUIT / Ctrl-C)
    Interrupted,
    /// Error generated on `WINDOW_BUFFER_SIZE_EVENT` / `SIGWINCH` signal
    Signal(Signal),
}

impl fmt::Display for ReadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Io(ref err) => err.fmt(f),
            Self::Eof => write!(f, "EOF"),
            Self::Interrupted => write!(f, "Interrupted"),
            Self::Signal(ref sig) => write!(f, "Signal({sig:?})"),
        }
    }
}

impl Error for ReadlineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::Io(ref err) => Some(err),
            Self::Eof => None,
            Self::Interrupted => None,
            Self::Signal(_) => None,
        }
    }
}

/// Signal received from terminal
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Signal {
    /// SIGINT
    #[cfg(unix)]
    Interrupt,
    /// SIGWINCH / `WINDOW_BUFFER_SIZE_EVENT`
    Resize,
}

impl From<io::Error> for ReadlineError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<io::ErrorKind> for ReadlineError {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Io(io::Error::from(kind))
    }
}

impl From<fmt::Error> for ReadlineError {
    fn from(err: fmt::Error) -> Self {
        Self::Io(io::Error::other(err))
    }
}

#[cfg(windows)]
impl From<std::char::DecodeUtf16Error> for ReadlineError {
    fn from(err: std::char::DecodeUtf16Error) -> Self {
        Self::Io(io::Error::new(io::ErrorKind::InvalidData, err))
    }
}
