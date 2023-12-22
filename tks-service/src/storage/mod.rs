use lazy_static::lazy_static;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::DirBuilder;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

use crate::settings::SETTINGS;

#[derive(Debug)]
struct ItemData {
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Item {
    #[serde(skip)]
    data: Option<ItemData>,

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    attributes: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Collection {
    name: String,
    items: Option<Vec<Item>>,
    aliases: Option<Vec<String>>,

    #[serde(skip)]
    path: OsString,
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

        let paths = std::fs::read_dir(storage.path.clone()).unwrap();
        for path in paths {
            let collection_path = path.unwrap().path();
            storage
                .collections
                .push(Self::load_collection(&collection_path).unwrap());
        }

        // look for the default collection and create it if it doesn't exist
        match storage.read_alias("default") {
            Ok(name) => name,
            Err(_) => {
                debug!("Creating default collection");
                let _ = storage.create_collection("default", &HashMap::new());
                for collection in storage.collections.iter_mut() {
                    if collection.name == "default" {
                        collection.aliases = Some(vec!["default".to_string()]);
                    }
                }
                "default".to_string()
            }
        };

        Ok(storage)
    }

    pub fn read_alias(&mut self, alias: &str) -> Result<String, std::io::Error> {
        self.collections
            .iter()
            .filter(|c| c.aliases.is_some())
            .find(|&c| c.aliases.as_ref().unwrap().contains(&alias.to_string()))
            .map(|c| c.name.clone())
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Alias '{}' not found", alias),
            ))
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
        collection_path.push(self.path.clone());
        collection_path.push(name);
        let coll = Collection::new(name, collection_path.as_os_str());
        Self::save_collection(&coll)?;
        self.collections.push(coll);
        debug!(
            "Created collection '{}' at path '{}'",
            name,
            collection_path.display()
        );
        Ok(())
    }

    fn save_collection(collection: &Collection) -> Result<(), std::io::Error> {
        assert!(!collection.path.is_empty());
        let _ = DirBuilder::new()
            .recursive(true)
            .create(collection.path.clone())?;
        let mut metadata_path = PathBuf::new();
        metadata_path.push(collection.path.clone());
        metadata_path.push("metadata.json");
        debug!(
            "Saving collection '{}' to path '{}'",
            collection.name,
            metadata_path.display()
        );
        let mut file = File::create(metadata_path)?;
        serde_json::to_writer_pretty(&mut file, collection)?;
        Ok(())
    }

    fn load_collection(path: &PathBuf) -> Result<Collection, std::io::Error> {
        let mut metadata_path = PathBuf::new();
        metadata_path.push(path);
        metadata_path.push("metadata.json");

        let mut file = File::open(metadata_path.file_name().unwrap())?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let collection: Collection = serde_json::from_str(&data)?;
        Ok(collection)
    }
}

impl Collection {
    fn new(name: &str, path: &OsStr) -> Collection {
        let collection = Collection {
            name: name.to_string(),
            path: path.to_os_string(),
            items: None,
            aliases: None,
        };

        collection
    }
}
