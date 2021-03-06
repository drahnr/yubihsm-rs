//! Errors that occur during sessions

use connector::{ConnectionError, ConnectionErrorKind};
use error::{Error, HsmErrorKind};
use serialization::SerializationError;
use session::{SessionError, SessionErrorKind};
use std::error::Error as StdError;

/// Session errors
pub type ClientError = Error<ClientErrorKind>;

/// Session error kinds
#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ClientErrorKind {
    /// Couldn't authenticate session
    #[fail(display = "authentication failed")]
    AuthFail,

    /// Session is closed
    #[fail(display = "session closed")]
    ClosedSessionError,

    /// Errors with the connection to the HSM
    #[fail(display = "connection error")]
    ConnectionError {
        /// Connection error kind
        kind: ConnectionErrorKind,
    },

    /// Couldn't create session
    #[fail(display = "couldn't create session")]
    CreateFailed,

    /// Errors originating in the HSM device
    #[fail(display = "HSM error: {}", kind)]
    DeviceError {
        /// HSM error kind
        kind: HsmErrorKind,
    },

    /// Protocol error occurred
    #[fail(display = "protocol error")]
    ProtocolError,

    /// Error response from HSM we can't further specify
    #[fail(display = "HSM error")]
    ResponseError,
}

// TODO: capture causes?
impl From<ConnectionError> for ClientError {
    fn from(err: ConnectionError) -> Self {
        let kind = ClientErrorKind::ConnectionError { kind: err.kind() };
        err!(kind, err.description())
    }
}

// TODO: capture causes?
impl From<SessionError> for ClientError {
    fn from(err: SessionError) -> Self {
        let kind = match err.kind() {
            SessionErrorKind::AuthFail => ClientErrorKind::AuthFail,
            SessionErrorKind::ClosedSessionError => ClientErrorKind::ClosedSessionError,
            SessionErrorKind::CreateFailed => ClientErrorKind::CreateFailed,
            SessionErrorKind::DeviceError { kind } => ClientErrorKind::DeviceError { kind },
            SessionErrorKind::ProtocolError
            | SessionErrorKind::CommandLimitExceeded
            | SessionErrorKind::MismatchError
            | SessionErrorKind::VerifyFailed => ClientErrorKind::ProtocolError,
            SessionErrorKind::ResponseError => ClientErrorKind::ResponseError,
        };

        err!(kind, err.description())
    }
}

// TODO: capture causes?
impl From<SerializationError> for ClientError {
    fn from(err: SerializationError) -> Self {
        err!(ClientErrorKind::ProtocolError, err.description())
    }
}
