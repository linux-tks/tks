use crate::settings::{Settings, Storage};
use crate::storage::collection::Collection;
use crate::storage::{SecretsHandler, StorageBackend, StorageBackendType};
use crate::tks_dbus::prompt_impl::PromptAction;
use crate::tks_error::TksError;
use homedir::my_home;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

pub struct PasswordStoreBackend {
    path: PathBuf,
    metadata_path: Option<PathBuf>,
}

/// Shares back-end with the password-store, aka `pass`, utility
///
/// This backend is located by default in ~/.password-store
/// Data is being managed using the `pass` utility, as documented by the
/// original authors here: https://www.passwordstore.org/
///
/// We consider that the entire storage is a single collection, and we map it
/// to the Secret Service `default` collection. We do not support adding any
/// other collection so the Service::CreateCollection method is not supported.
impl PasswordStoreBackend {
    pub(crate) fn new(settings: Storage) -> Result<PasswordStoreBackend, TksError> {
        log::trace!("enter: password_store_backend({:?})", settings);
        // default to user's ~/.password-store directory
        let path = settings.path.map_or_else(
            || {
                my_home().ok().unwrap().unwrap() // this has to potential to break on Windows, but we do not support Windows!
            },
            PathBuf::from,
        );
        log::info!("path: {:?}", path);
        let mut b = Self {
            path,
            metadata_path: None,
        };
        b.create_or_update_metadata()?;

        Ok(b)
    }

    /// we maintain a *fake* metadata file, to support the internal data
    /// structures
    fn create_or_update_metadata(&mut self) -> Result<(), TksError> {
        let mut metadata_path = PathBuf::new();
        let path: OsString = xdg::BaseDirectories::with_prefix(Settings::XDG_DIR_NAME)?
            .create_data_directory("password-store")?.into();
        metadata_path.push(path.clone());
        metadata_path.push("metadata");
        // create if not exists ~/.local/share/io.linux-tks/password-store/metadata
        let _ = fs::DirBuilder::new()
            .recursive(true)
            .create(metadata_path.clone())?;
        self.metadata_path = Some(metadata_path.clone());

        // now check if we have the default collection created here
        let mut collection_path = PathBuf::new();
        collection_path.push(metadata_path);
        collection_path.push("default");

        if !fs::exists(collection_path.clone())? {
            let coll = Collection::new(crate::storage::DEFAULT_NAME,
                                       &collection_path, &self.path)?;
            let metadata = serde_json::to_string(&coll)?;
            self.save_collection_metadata(&coll.path, &metadata)?;
        }

        Ok(())
    }
}

impl PasswordStoreBackend {}

impl StorageBackend for PasswordStoreBackend {
    fn get_kind(&self) -> StorageBackendType {
        StorageBackendType::PasswordStore
    }

    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError> {
        // // we enumerate all the directories and return the paths to the leaf directories
        // let dirs = fs::read_dir(self.path.clone())?
        //     .into_iter()
        //     .filter(|entry| entry.is_ok())
        //     .map(|entry| entry.unwrap().path())
        //     .filter(|path| path.is_dir())
        //     .collect();
        Ok(vec![self.metadata_path.clone().unwrap()])
    }

    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError> {
        Err(TksError::NotSupported(
            "password-store backend does not support creating new collections",
        ))
    }

    fn collection_items_path(&self, name: &str) -> Result<PathBuf, TksError> {
        todo!()
    }

    fn get_secrets_handler(&mut self) -> Result<Box<dyn SecretsHandler + '_>, TksError> {
        todo!()
    }

    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        todo!()
    }

    fn create_unlock_action(
        &mut self,
        coll_uuid: &Uuid,
        coll_name: &str,
    ) -> Result<PromptAction, TksError> {
        todo!()
    }

    fn is_locked(&self) -> Result<bool, TksError> {
        todo!()
    }

    fn save_collection_metadata(
        &mut self,
        coll_path: &PathBuf,
        x: &String,
    ) -> Result<(), TksError> {
        todo!()
    }

    fn save_collection_items(
        &mut self,
        coll_items_path: &PathBuf,
        x: &String,
        x0: &String,
    ) -> Result<(), TksError> {
        todo!()
    }

    fn load_collection_items(
        &self,
        collection: &Collection,
        metadata: &String,
    ) -> Result<Vec<u8>, TksError> {
        todo!()
    }
}
