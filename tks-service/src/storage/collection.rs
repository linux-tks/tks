use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::PathBuf;
use tokio::task::JoinHandle;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use log::{debug, error, trace};
use crate::storage::{CollectionSecrets, DEFAULT_NAME, STORAGE};
use crate::tks_dbus::session_impl::Session;
use crate::tks_error::TksError;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ItemData {
    uuid: Uuid,
    data: Vec<u8>,
    pub content_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Item {
    pub label: String,
    pub created: u64,
    pub modified: u64,
    pub attributes: HashMap<String, String>,
    pub id: ItemId,

    // when Item is locked, this is None
    #[serde(skip)]
    pub data: Option<ItemData>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ItemId {
    pub uuid: Uuid,
    #[serde(skip)]
    pub collection_uuid: Uuid,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Collection {
    schema_version: u8,
    pub uuid: Uuid,
    pub default: bool,
    pub name: String,
    pub items: Vec<Item>,
    pub aliases: Option<Vec<String>>,
    pub created: u64,
    pub modified: u64,

    #[serde(skip)]
    pub(crate) path: PathBuf,
    #[serde(skip)]
    pub(crate) items_path: PathBuf,
    #[serde(skip)]
    pub locked: bool,
    #[serde(skip)]
    pending_async: Option<JoinHandle<()>>,
}

impl Collection {
    pub(crate) fn new(name: &str, path: &PathBuf, items_path: &PathBuf) -> Result<Collection, TksError> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting system time: {}", e),
                )
            })?
            .as_secs()
            .into();
        let collection = Collection {
            uuid: Uuid::new_v4(),
            default: DEFAULT_NAME == name,
            schema_version: 1,
            name: name.to_string(),
            path: path.clone(),
            items_path: items_path.clone(),
            items: Vec::new(),
            aliases: None,
            locked: true,
            created: ts,
            modified: ts,
            pending_async: None,
        };

        Ok(collection)
    }

    pub fn create_item(
        &mut self,
        label: &str,
        properties: HashMap<String, String>,
        secret: (&Session, Vec<u8>, Vec<u8>, String),
        replace: bool,
        sender: String,
    ) -> Result<ItemId, TksError> {
        if self.locked {
            return Err(TksError::PermissionDenied);
        }
        let secret_session = secret.0;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting system time: {}", e),
                )
            })?
            .as_secs()
            .into();
        let uuid = Uuid::new_v4();
        let item = Item {
            label: label.to_string(),
            created: ts,
            modified: ts,
            data: Some(ItemData {
                uuid,
                data: match secret_session.decrypt(&secret.1, &secret.2, sender) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Cannot decrypt secret: {}", e);
                        return Err(TksError::CryptoError);
                    }
                },
                content_type: secret.3,
            }),
            id: ItemId {
                collection_uuid: self.uuid,
                uuid,
            },
            attributes: properties,
        };
        let item = if let Some(index) = self.items.iter().position(|i| {
            i.attributes == item.attributes
                && match (&i.data, &item.data) {
                    (Some(d1), Some(d2)) => {
                        d1.content_type == d2.content_type && d1.data == d2.data
                    }
                    (None, None) => true,
                    _ => false,
                }
        }) {
            if replace {
                self.items[index] = item;
                self.items.get(index).unwrap()
            } else {
                return Err(TksError::Duplicate);
            }
        } else {
            self.items.push(item);
            self.items.last().unwrap()
        };
        let item_id = item.id.clone();
        Ok(item_id)
    }

    pub fn get_item(&self, uuid: &Uuid) -> Result<&Item, TksError> {
        self.items
            .iter()
            .find(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
    }

    pub fn get_item_mut(&mut self, uuid: &Uuid) -> Result<&mut Item, TksError> {
        self.items
            .iter_mut()
            .find(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
    }

    pub fn delete_item(&mut self, uuid: &Uuid) -> Result<Item, TksError> {
        if self.locked {
            return Err(TksError::PermissionDenied);
        }
        self.items
            .iter()
            .position(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
            .and_then(|i| {
                let older = self.items.swap_remove(i);
                Ok(older)
            })
    }

    pub(crate) fn get_secrets(&self) -> CollectionSecrets {
        CollectionSecrets {
            items: self
                .items
                .iter()
                .map(|i| i.data.as_ref().unwrap().clone())
                .collect(),
        }
    }

    pub fn unlock(&mut self) -> Result<(), TksError> {
        if !self.locked || self.items.is_empty() {
            self.locked = false;
            return Ok(());
        }

        let collection_uuid = self.uuid.clone();
        let items_path = self.items_path.clone();

        self.pending_async = Some(tokio::spawn(async move {
            debug!("Performing collection unlock: {}", collection_uuid);
            let data = STORAGE
                .lock()
                .unwrap()
                .unlock_items(&items_path)
                .ok()
                .or(Some("".to_string()))
                .unwrap();
            let collection_secrets: CollectionSecrets = serde_json::from_str(&data)
                .ok()
                .or(Some(CollectionSecrets::new()))
                .unwrap();

            let _ = STORAGE
                .lock()
                .unwrap()
                .modify_collection(&collection_uuid, |c| {
                    c.items.iter_mut().for_each(|item| {
                        let _ = collection_secrets
                            .items
                            .iter()
                            .find(|s| s.uuid == item.id.uuid)
                            .ok_or(std::io::Error::new(
                                // TODO: maybe we should put the service in a fail state if we can't unlock a collection
                                std::io::ErrorKind::NotFound,
                                format!("Secrets file does not contain secret for item "),
                            ))
                            .and_then(|s| {
                                item.data = Some(s.clone());
                                Ok(())
                            });
                    });
                    c.locked = false;
                    Ok(())
                });
        }));
        Ok(())
    }
    pub fn lock(&mut self) -> Result<(), TksError> {
        self.locked = true;
        self.items.iter_mut().for_each(|item| item.data = None);
        Ok(())
    }
}

impl Item {
    pub fn get_secret(
        &self,
        session: &Session,
        sender: String,
    ) -> Result<(String, Vec<u8>, Vec<u8>, String), std::io::Error> {
        trace!("get_secret called on '{}'", self.label);
        let data = self.data.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("Item is locked"))
        })?;

        let (iv, secret) = session.encrypt(&data.data, sender).map_err(|e| {
            error!("Error encrypting secret: {}", e);
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Data cannot be prepared"),
            )
        })?;
        Ok(("".to_string(), iv, secret, data.content_type.clone()))
    }
    pub fn set_secret(
        &mut self,
        session: &Session,
        parameters: Vec<u8>,
        value: &Vec<u8>,
        content_type: String,
        sender: String,
    ) -> Result<(), TksError> {
        trace!("set_secret called on '{}'", self.label);
        self.data = Some(ItemData {
            uuid: self.id.uuid,
            data: session.decrypt(&parameters, value, sender)?,
            content_type,
        });
        Ok(())
    }
}

impl CollectionSecrets {
    pub fn new() -> CollectionSecrets {
        CollectionSecrets { items: Vec::new() }
    }
}
