use lazy_static::lazy_static;
use log::{debug, error};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::DirBuilder;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

use crate::settings::SETTINGS;

struct Item {
    attributes: HashMap<String, String>,
    data: Option<Vec<u8>>,
}

struct Collection {
    path: OsString,
    items: Option<Vec<Item>>,
}

pub struct Storage {
    path: OsString,
    collections: Vec<Collection>,
}

lazy_static! {
    pub static ref STORAGE: Arc<Mutex<Storage>> = Arc::new(Mutex::new(Storage::new().unwrap()));
}

impl Storage {
    fn new() -> Result<Self, std::io::Error> {
        let mut storage = Storage {
            path: OsString::from(SETTINGS.lock().unwrap().storage.path.clone()),
            collections: Vec::new(),
        };
        // check if the storage directory exists
        // if not, create it
        let _ = DirBuilder::new()
            .recursive(true)
            .create(storage.path.clone())?;

        // enumerate files in the storage directory
        // for each file, create a new StorageFile
        // add the StorageFile to the Storage
        // return the Storage
        let paths = std::fs::read_dir(storage.path.clone()).unwrap();
        for path in paths {
            let path = path.unwrap().path();
            storage
                .collections
                .push(Collection::new(path.file_name().unwrap()));
        }

        Ok(storage)
    }

    /// Create a new collection
    ///
    /// # Arguments
    /// * `name` - The name of the collection
    /// * `properties` - A HashMap of properties to set on the collection; this version ignores
    /// these properties and this is allowd by the spec
    /// # Returns
    /// * `Ok(())` - The collection was created successfully
    /// * `Err(std::io::Error)` - There was an error creating the collection
    pub fn create_collection(
        &mut self,
        name: &str,
        _properties: &HashMap<String, String>,
    ) -> Result<(), std::io::Error> {
        let mut collection_path = PathBuf::new();
        collection_path.push(SETTINGS.lock().unwrap().storage.path.clone());
        collection_path.push(name);
        let _ = DirBuilder::new()
            .recursive(true)
            .create(collection_path.clone())?;
        let coll = Collection::new(collection_path.file_name().unwrap());
        self.collections.push(coll);
        debug!(
            "Created collection {} at path {}",
            name,
            collection_path.display()
        );
        Ok(())
    }
}

impl Collection {
    fn new(path: &OsStr) -> Collection {
        let collection = Collection {
            path: path.to_os_string(),
            items: None,
        };

        collection
    }
}
