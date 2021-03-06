//! Error types for `yubihsm-connector`

#[cfg(feature = "usb")]
use libusb;
use std::num::ParseIntError;
use std::str::Utf8Error;
use std::{fmt, io};

use error::Error;

/// `yubihsm-connector` related errors
pub type ConnectionError = Error<ConnectionErrorKind>;

/// `yubihsm-connector` related error kinds
#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ConnectionErrorKind {
    /// Address provided was not valid
    #[fail(display = "invalid address")]
    AddrInvalid,

    /// Access denied
    #[fail(display = "access denied")]
    AccessDenied,

    /// YubiHSM2 is busy (in use by another client / process)
    #[fail(display = "device already in use")]
    DeviceBusyError,

    /// Couldn't connect to the YubiHSM2
    #[fail(display = "connection failed")]
    ConnectionFailed,

    /// Input/output error
    #[fail(display = "I/O error")]
    IoError,

    /// Error making request
    #[fail(display = "invalid request")]
    RequestError,

    /// `yubihsm-connector` sent bad response
    #[fail(display = "bad connector response")]
    ResponseError,

    /// USB operation failed
    #[cfg(feature = "usb")]
    #[fail(display = "USB error")]
    UsbError,
}

impl From<fmt::Error> for ConnectionError {
    fn from(err: fmt::Error) -> Self {
        err!(ConnectionErrorKind::IoError, err.to_string())
    }
}

impl From<io::Error> for ConnectionError {
    fn from(err: io::Error) -> Self {
        err!(ConnectionErrorKind::IoError, err.to_string())
    }
}

#[cfg(feature = "usb")]
impl From<libusb::Error> for ConnectionError {
    fn from(err: libusb::Error) -> ConnectionError {
        match err {
            libusb::Error::Access => err!(ConnectionErrorKind::AccessDenied, "{}", err),
            libusb::Error::Io => err!(ConnectionErrorKind::IoError, "{}", err),
            _ => err!(ConnectionErrorKind::UsbError, "{}", err),
        }
    }
}

impl From<ParseIntError> for ConnectionError {
    fn from(err: ParseIntError) -> Self {
        err!(ConnectionErrorKind::ResponseError, err.to_string())
    }
}

impl From<Utf8Error> for ConnectionError {
    fn from(err: Utf8Error) -> Self {
        err!(ConnectionErrorKind::ResponseError, err.to_string())
    }
}
