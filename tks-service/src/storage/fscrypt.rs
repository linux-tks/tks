//!
//! Warning: EXPERIMENTAL
//!
//! This is a fscrypt back-end experiment
//!
use std::path::PathBuf;
use uuid::Uuid;
use log::{trace, warn};
use std::ffi::OsString;
use std::fs::DirBuilder;
use crate::storage::{CollectionUnlockAction, StorageBackend, StorageBackendType};
use crate::tks_dbus::prompt_impl::{PromptAction, PromptWithPinentry, TksFscryptPrompt};
use crate::tks_error::TksError;

pub struct FSCryptBackend {
    path: OsString,
    metadata_path: OsString,
    items_path: OsString,
    commissioned: bool,
}

impl FSCryptBackend {
    pub(crate) fn new(path: OsString) -> Result<FSCryptBackend, TksError> {
        warn!("Initializing EXPERIMENTAL fscrypt storage at {:?}", path);
        let mut metadata_path = PathBuf::new();
        metadata_path.push(path.clone());
        metadata_path.push("metadata");
        let _ = DirBuilder::new()
            .recursive(true)
            .create(metadata_path.clone())?;

        let mut items_path = PathBuf::new();
        items_path.push(path.clone());
        items_path.push("items");
        let _ = DirBuilder::new()
            .recursive(true)
            .create(items_path.clone())?;

        let commissioned = false;
        let backend = FSCryptBackend {
            path,
            metadata_path: metadata_path.into(),
            items_path: items_path.into(),
            commissioned,
        };
        Ok(backend)
    }
}

impl StorageBackend for FSCryptBackend {
    fn get_kind(&self) -> StorageBackendType {
        StorageBackendType::FSCrypt
    }

    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError> {
        Ok(std::fs::read_dir(self.metadata_path.clone())?
            .into_iter()
            .filter(|e| e.is_ok())
            .map(|p| p.unwrap().path())
            .filter(|p| p.is_file())
            .collect())
    }

    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError> {
        let mut collection_path = PathBuf::new();
        collection_path.push(self.metadata_path.clone());
        collection_path.push(name);
        let mut items_path = PathBuf::new();
        items_path.push(self.items_path.clone());
        items_path.push(name);
        Ok((collection_path, items_path))
    }

    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        if !items_path.starts_with(self.items_path.clone()) {
            return Err(TksError::InternalError(
                "Items path not within the correct directory",
            ));
        }
        if !self.commissioned {
            return Err(TksError::BackendError(format!(
                "Storage in directory {:?} is not commissioned",
                self.items_path
            )));
        }
        Ok("".to_string())
    }

    fn create_unlock_action(&self, coll_uuid: &Uuid) -> Result<PromptAction, TksError> {
        trace!("create_onlock_prompt for {:?}", coll_uuid);
        Ok(TksFscryptPrompt::new(coll_uuid))
    }
}
