#![feature(iter_array_chunks)]

extern crate core;

use crate::storage::Storage;
use dbus::MethodErr;
use log::error;
use openssl::error::ErrorStack;
use std::sync::{MutexGuard, PoisonError};

pub mod settings;
pub mod storage;
pub mod tks_dbus;

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
}

impl std::fmt::Display for TksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TksError::ParameterError => write!(f, "Parameter error"),
            TksError::CryptoError => write!(f, "Crypto error"),
            TksError::IOError(e) => write!(f, "IO error: {}", e),
            TksError::NotFound(_) => write!(f, "Not found"),
            TksError::SerializationError(_) => write!(f, "Serialization error"),
            TksError::PermissionDenied => write!(f, "Access denied"),
            TksError::Duplicate => write!(f, "Duplicate element"),
            TksError::LockingError => write!(f, "Locking error"),
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
