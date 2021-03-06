//! **yubihsm.rs**: pure Rust client for YubiHSM2 hardware security modules
//!
//! ## Prerequisites
//!
//! This crate builds on Rust 1.27+ and by default uses SIMD features
//! which require the following `RUSTFLAGS`:
//!
//! `RUSTFLAGS=-Ctarget-feature=+aes`
//!
//! You can configure your `~/.cargo/config` to always pass these flags:
//!
//! ```toml
//! [build]
//! rustflags = ["-Ctarget-feature=+aes"]
//! ```
//!
//! # Getting Started
//!
//! The following documentation describes the most important parts of this crate's API:
//!
//! * [yubihsm::connector]: methods of connecting to a YubiHSM (USB or HTTP via [yubihsm-connector])
//! * [yubihsm::Client]: client providing wrappers for YubiHSM [commands].
//!
//! # Example
//!
//! The following is an example of how to create a [yubihsm::Client] by
//! connecting via USB, and then performing an Ed25519 signature:
//!
//! ```no_build
//! extern crate yubihsm;
//! use yubihsm::{Client, Credentials, UsbConnector};
//!
//! // Connect to the first YubiHSM2 we detect
//! let connector = UsbConnector::default();
//!
//! // Default auth key ID and password for YubiHSM2
//! // NOTE: DON'T USE THIS IN PRODUCTION!
//! let credentials = Credentials::default();
//!
//! // Connect to the HSM and authenticate with the given credentials
//! let mut hsm_client = Client::open(connector, credentials, true).unwrap();
//!
//! // Note: You'll need to create this key first. Run the following from yubihsm-shell:
//! // `generate asymmetric 0 100 ed25519_test_key 1 asymmetric_sign_eddsa ed25519`
//! let signature = hsm_client.sign_ed25519(100, "Hello, world!").unwrap();
//! println!("Ed25519 signature: {:?}", signature);
//! ```
//!
//! [yubihsm::connector]: https://docs.rs/yubihsm/latest/yubihsm/connector/index.html
//! [yubihsm::Client]: https://docs.rs/yubihsm/latest/yubihsm/client/struct.Client.html
//! [commands]: https://developers.yubico.com/YubiHSM2/Commands/
//! [yubihsm-connector]: https://developers.yubico.com/YubiHSM2/Component_Reference/yubihsm-connector/

#![crate_name = "yubihsm"]
#![crate_type = "rlib"]
#![cfg_attr(clippy, feature(tool_lints))]
#![deny(warnings, missing_docs, trivial_casts, trivial_numeric_casts)]
#![deny(unsafe_code, unused_import_braces, unused_qualifications)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/tendermint/yubihsm-rs/master/img/logo.png",
    html_root_url = "https://docs.rs/yubihsm/0.18.1"
)]

extern crate aes;
#[macro_use]
extern crate bitflags;
extern crate block_modes;
extern crate byteorder;
extern crate cmac;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[cfg(feature = "hmac")]
extern crate hmac;
#[cfg(feature = "usb")]
#[macro_use]
extern crate lazy_static;
#[cfg(feature = "usb")]
extern crate libusb;
#[macro_use]
extern crate log;
#[cfg(feature = "pbkdf2")]
extern crate pbkdf2;
extern crate rand;
#[cfg(feature = "mockhsm")]
extern crate ring;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[cfg(feature = "sha2")]
extern crate sha2;
extern crate subtle;
#[cfg(feature = "mockhsm")]
extern crate untrusted;
extern crate uuid;
extern crate zeroize;

/// Error types
#[macro_use]
pub mod error;

/// Serde-powered serializers for the HSM wire format
#[macro_use]
mod serialization;

/// Cryptographic algorithms supported by the HSM
pub mod algorithm;

/// Auditing options (for use with the `get_option` and `put_option` command)
pub(crate) mod audit;

/// Authentication keys used to establish encrypted sessions with the HSM
pub mod auth_key;

/// Object attributes specifying which operations are allowed to be performed
pub mod capability;

/// YubiHSM client: main functionality of this crate
pub mod client;

/// Commands supported by the HSM.
///
/// Functions defined in the `yubihsm::command` module are reimported
/// and available from the toplevel `yubihsm` module as well.
///
/// For more information, see:
/// <https://developers.yubico.com/YubiHSM2/Commands/>
pub mod command;

/// Methods of connecting to a YubiHSM2:
///
/// - [HttpConnector]: communicates with HSM via the `yubihsm-connector` service's HTTP API
/// - [UsbConnector]: communicates with the HSM directly via USB using `libusb`.
///
/// Additionally, [MockHsm] implements the `Connector` API and can be used as a drop-in replacement
/// in places where you would like a simulated HSM for testing (e.g. CI).
///
/// [HttpConnector]: https://docs.rs/yubihsm/latest/yubihsm/connector/http/struct.HttpConnector.html
/// [UsbConnector]: https://docs.rs/yubihsm/latest/yubihsm/connector/usb/struct.UsbConnector.html
/// [MockHsm]: https://docs.rs/yubihsm/latest/yubihsm/mockhsm/struct.MockHsm.html
pub mod connector;

/// Credentials used to authenticate to the HSM (key ID + `AuthKey`).
pub mod credentials;

/// Logical partitions within the HSM, allowing several applications to share the device.
pub mod domain;

/// Simulation of the HSM for integration testing.
#[cfg(feature = "mockhsm")]
pub mod mockhsm;

/// Authenticated/encrypted sessions with the HSM.
///
/// For more information, see:
/// <https://developers.yubico.com/YubiHSM2/Concepts/Session.html>
pub mod session;

/// Objects stored in the HSM.
///
/// For more information, see:
/// <https://developers.yubico.com/YubiHSM2/Concepts/Object.html>
pub mod object;

/// Responses to command sent from the HSM.
pub mod response;

/// HSM serial numbers.
mod serial_number;

/// Object wrapping support, i.e. encrypt objects from one HSM to another.
pub mod wrap;

pub use algorithm::*;
pub use audit::AuditOption;
pub use auth_key::{AuthKey, AUTH_KEY_SIZE};
pub use capability::Capability;
pub use client::{Client, ClientError};
pub use command::CommandCode;
#[cfg(feature = "http")]
pub use connector::http::{HttpConfig, HttpConnector};
#[cfg(feature = "usb")]
pub use connector::usb::{UsbConfig, UsbConnector};
pub use connector::{Connection, ConnectionError, Connector};
pub use credentials::Credentials;
pub use domain::Domain;
pub use error::*;
#[cfg(feature = "mockhsm")]
pub use mockhsm::MockHsm;
pub use object::*;
pub use response::ResponseCode;
pub use serial_number::SerialNumber;
pub use session::SessionId;
pub use uuid::Uuid;
pub use wrap::{WrapMessage, WrapNonce};
