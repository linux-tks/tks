//!
//! Tks specific backend using the AES/GCM item secrets encryption
//!
use std::ffi::OsString;
use std::path::PathBuf;
use dbus::Path;
use uuid::Uuid;
use StorageBackendType::TksGcm;
use crate::storage::{StorageBackend, StorageBackendType};
use crate::tks_error::TksError;

pub struct TksGcmBackend {

}

impl TksGcmBackend {
    pub(crate) fn new(p0: OsString) -> TksGcmBackend {
        todo!()
    }
}

impl TksGcmBackend {

}

impl StorageBackend for TksGcmBackend {
    fn get_kind(&self) -> StorageBackendType {
        TksGcm
    }

    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError> {
        todo!()
    }

    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError> {
        todo!()
    }

    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        todo!()
    }

    fn create_unlock_prompt(&self, coll_uuid: &Uuid) -> Result<Path<'static>, TksError> {
        todo!()
    }
}