use log::error;
use config::ConfigError;
use std::sync::{MutexGuard, PoisonError};
use dbus::MethodErr;
use openssl::error::ErrorStack;
use pinentry::Error;
use crate::storage;
use crate::storage::Storage;
use homedir::GetHomeError;

#[derive(Debug)]
pub enum TksError {
    ParameterError,
    NotFound(Option<String>),
    CryptoError,
    IOError(std::io::Error),
    SerializationError(String),
    PermissionDenied,
    Duplicate,
    LockingError,
    ConfigurationError(String),
    InternalError(&'static str),
    BackendError(String),
    NoPinentryBinaryFound,
    PinentryError(Error),
    ItemNotFound,
    DBusError(String),
    ContextError(&'static str),
    GetHomeError(GetHomeError),
    NotSupported(&'static str),
}

impl std::fmt::Display for TksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TksError::ParameterError => write!(f, "Parameter error"),
            TksError::CryptoError => write!(f, "Crypto error"),
            TksError::IOError(e) => write!(f, "IO error: {}", e),
            TksError::NotFound(_) => write!(f, "Not found"),
            TksError::SerializationError(s) => write!(f, "Serialization error: {}", s),
            TksError::PermissionDenied => write!(f, "Access denied"),
            TksError::Duplicate => write!(f, "Duplicate element"),
            TksError::LockingError => write!(f, "Locking error"),
            TksError::ConfigurationError(x) => write!(f, "Configuration error: {}", x),
            TksError::InternalError(x) => { write!(f, "Internal error: {}", x)},
            TksError::BackendError(x) => { write!(f, "Backend error: {}", x)},
            TksError::NoPinentryBinaryFound => { write!{f, "No pinentry binary found, please install it."}},
            TksError::PinentryError(e) => { write!{f, "Pinentry returned error {}", e}},
            TksError::ItemNotFound => { write!{f, "Item not found upon unlocking collection. Maybe data is corrupted?"}},
            TksError::DBusError(x) => { write!(f, "DBusError: {}", x)},
            TksError::ContextError(x) => { write!(f, "ContextError: {}", x)},
            TksError::GetHomeError(x) => { write!(f, "GetHomeError: {}", x)},
            TksError::NotSupported(x) => { write!(f, "Not supported: {}", x)},
        }
    }
}

impl From<std::io::Error> for TksError {
    fn from(e: std::io::Error) -> Self {
        error!("io error: {:?}", e);
        TksError::IOError(e)
    }
}

impl From<ErrorStack> for TksError {
    fn from(e: ErrorStack) -> Self {
        error!("openssl error: {:?}", e);
        TksError::CryptoError
    }
}

impl From<serde_json::Error> for TksError {
    fn from(e: serde_json::Error) -> Self {
        error!("serde_json error: {}", e);
        TksError::SerializationError(e.to_string())
    }
}

impl From<PoisonError<std::sync::MutexGuard<'_, storage::Storage>>> for TksError {
    fn from(e: PoisonError<MutexGuard<'_, Storage>>) -> Self {
        error!("Unexpected locking condition: {}", e);
        TksError::LockingError
    }
}

impl From<TksError> for MethodErr {
    fn from(e: TksError) -> Self {
        dbus::MethodErr::failed(&e.to_string())
    }
}

impl From<xdg::BaseDirectoriesError> for TksError {
    fn from(e: xdg::BaseDirectoriesError) -> Self {
        error!("BaseDirectoriesError {}", e);
        TksError::ConfigurationError(e.to_string())
    }
}

impl From<ConfigError> for TksError {
    fn from(e: ConfigError) -> Self {
        error!("ConfigError {}", e);
        TksError::ConfigurationError(e.to_string())
    }
}

impl From<pinentry::Error> for TksError {
    fn from(e: Error) -> Self {
        TksError::PinentryError(e)
    }
}

impl From<dbus::Error> for TksError {
    fn from(e: dbus::Error) -> Self { TksError::DBusError(e.to_string()) }
}

impl From<GetHomeError> for TksError {
    fn from(e: GetHomeError) -> Self { TksError::GetHomeError(e) }
}