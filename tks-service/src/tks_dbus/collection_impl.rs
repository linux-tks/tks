use crate::register_object;
use crate::storage::Collection;
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::collection::register_org_freedesktop_secret_collection;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollection;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemCreated;
use crate::tks_dbus::fdo::prompt::register_org_freedesktop_secret_prompt;
use crate::tks_dbus::item_impl::ItemHandle;
use crate::tks_dbus::prompt_impl::PromptHandle;
use crate::tks_dbus::prompt_impl::PromptImpl;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::DBusHandlePath::MultiplePaths;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::MESSAGE_SENDER;
use crate::tks_dbus::{sanitize_string, DBusHandlePath};
use arg::cast;
use dbus::arg;
use dbus::arg::RefArg;
use dbus::message::SignalArgs;
use lazy_static::lazy_static;
use log::{debug, error, trace, warn};
use pinentry::ConfirmationDialog;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Default, Clone)]
pub struct CollectionHandle {
    pub uuid: Uuid,
    pub default: bool,
    pub paths: Vec<dbus::Path<'static>>,
}

lazy_static! {
    pub static ref COLLECTION_HANDLES: Arc<Mutex<HashMap<Uuid, CollectionHandle>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

impl CollectionHandle {
    fn new(uuid: &Uuid, default: bool) -> CollectionHandle {
        let mut handle = CollectionHandle {
            uuid: uuid.clone(),
            default,
            paths: vec![dbus::Path::from(format!(
                "/org/freedesktop/secrets/collection/{}",
                sanitize_string(&uuid.to_string()).as_str()
            ))],
        };
        default.then(|| {
            // the default path should always be kept the first in the vector
            handle.paths.insert(
                0,
                dbus::Path::from("/org/freedesktop/secrets/aliases/default"),
            );
        });
        let handle_clone = handle.clone();
        register_object!(register_org_freedesktop_secret_collection, handle_clone);
        handle
    }
}

impl From<&Collection> for CollectionHandle {
    fn from(collection: &Collection) -> CollectionHandle {
        let uuid = collection.uuid;
        let is_new = !COLLECTION_HANDLES.lock().unwrap().contains_key(&uuid);
        is_new.then(|| {
            COLLECTION_HANDLES
                .lock()
                .unwrap()
                .insert(uuid.clone(), CollectionHandle::new(&uuid, collection.default));
        });
        COLLECTION_HANDLES
            .lock()
            .unwrap()
            .get(&uuid)
            .unwrap()
            .clone()
    }
}

impl From<&Uuid> for CollectionHandle {
    fn from(uuid: &Uuid) -> CollectionHandle {
        let is_new = !COLLECTION_HANDLES.lock().unwrap().contains_key(&uuid);
        is_new.then(|| {
            COLLECTION_HANDLES
                .lock()
                .unwrap()
                .insert(uuid.clone(), CollectionHandle::new(uuid, false));
        });
        COLLECTION_HANDLES
            .lock()
            .unwrap()
            .get(&uuid)
            .unwrap()
            .clone()
    }
}

impl DBusHandle for CollectionHandle {
    fn path(&self) -> DBusHandlePath {
        warn!("CollectionHandle::path() called");
        MultiplePaths(self.paths.clone())
    }
}

impl OrgFreedesktopSecretCollection for CollectionHandle {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        debug!("delete called on '{}'", self.uuid);
        // TODO: implement this when prompts are implemented
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid, |collection| {
                Ok(collection
                    .items
                    .iter()
                    .filter(|item| item.attributes == attributes)
                    .map(|item| ItemHandle::from(item).path().into())
                    .collect::<Vec<dbus::Path>>())
            })
            .map_err(|e| {
                dbus::MethodErr::failed(&format!("Error searching items for collection: {}", e))
            })
    }
    // d-feet example call:
    // {"org.freedesktop.Secret.Item.Label":GLib.Variant('s',"test"), "org.freedesktop.Secret.Item.Attributes":GLib.Variant("a{sv}",{"prop1":GLib.Variant('s',"val1"),"prop2":GLib.Variant('s',"val2")})}, ("/",[],[],""),0
    fn create_item(
        &self,
        properties: arg::PropMap,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        replace: bool,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        trace!(
            "create_item properties: {:?}, secret: ({:?})",
            properties,
            secret
        );
        let item_label = properties
            .get("org.freedesktop.Secret.Item.Label")
            .ok_or_else(|| dbus::MethodErr::failed(&"No label specified"))
            .and_then(|x| {
                cast::<String>(&x.0)
                    .ok_or_else(|| dbus::MethodErr::failed(&"Label is not a string"))
            })
            .and_then(|s| Ok(s.to_string()))?;
        // let mut errors = Vec::new();
        let item_attributes_v = properties
            .get("org.freedesktop.Secret.Item.Attributes")
            .ok_or_else(|| {
                dbus::MethodErr::failed(&format!(
                    "Error creating item: {}",
                    "No attributes specified"
                ))
            })?;
        item_attributes_v
            .0
            .as_iter()
            .unwrap()
            .for_each(|x| debug!("x: {:?}", x));
        let item_attributes = item_attributes_v
            .0
            .as_iter()
            .unwrap()
            .array_chunks()
            .map(|a: [_; 2]| (a[0].as_str().unwrap().into(), a[1].as_str().unwrap().into()))
            .collect::<HashMap<String, String>>();
        let session_id = secret
            .0
            .split('/')
            .last()
            .unwrap()
            .parse::<usize>()
            .map_err(|_| dbus::MethodErr::failed(&"Invalid session ID"))?;
        if let Ok(locked) = self.locked() {
            if locked {
                debug!(
                    "Collection '{}' is locked, now preparing prompt to unlock",
                    self.uuid
                );
                // NOTE: here we have a confirmation dialog, but really we should
                // unlock the collection depending on the disk encryption method;
                // TKS's preferred method would be to use a TPM, unlocked via the
                // pam module, but we should also support unlocking with a passphrase.
                // For the moment, we'll just use a confirmation dialog so we can test the rest of the prompt code.
                if let Some(mut confirmation) = ConfirmationDialog::with_default_binary() {
                    confirmation.with_ok("Yes").with_timeout(10);
                    let uuid = self.uuid.clone();
                    let self_clone = self.clone();
                    let prompt = PromptImpl::new(
                        confirmation,
                        format!("Unlock collection '{}'?", self.uuid).clone(),
                        move || {
                            debug!("Prompt confirmed");
                            let item_attributes = item_attributes.clone();
                            let item_label = item_label.clone();
                            STORAGE.lock().unwrap().with_collection(
                                uuid,
                                |collection| -> Result<(), std::io::Error> {
                                    collection.unlock()?;
                                    trace!("Creating item after collection unlock");
                                    CollectionHandle::create_item(
                                        self_clone.uuid.clone(),
                                        secret.clone(),
                                        replace,
                                        item_label,
                                        item_attributes,
                                        session_id,
                                    )
                                    .map(|_| ())
                                },
                            )
                        },
                        None,
                    );
                    let prompt_path = prompt.path();
                    register_object!(
                        register_org_freedesktop_secret_prompt::<PromptHandle>,
                        prompt
                    );
                    return Ok((dbus::Path::from("/"), prompt_path.into()));
                } else {
                    error!("Error creating confirmation dialog. Do you have pinentry installed?");
                    return Err(dbus::MethodErr::failed(
                        &"Error creating confirmation dialog",
                    ));
                };
            }
        } else {
            error!("Unexpected error occured");
            return Err(dbus::MethodErr::failed(&"Not found"));
        }
        CollectionHandle::create_item(
            self.uuid,
            secret,
            replace,
            item_label,
            item_attributes,
            session_id,
        )
        .map_err(move |e| {
            error!("Error creating item: {}", e);
            dbus::MethodErr::failed(&format!("Error creating item: {}", e))
        })
    }
    fn items(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid.clone(), |collection| {
                Ok(collection
                    .items
                    .iter()
                    .map(|item| {
                        let ref ih = ItemHandle::from(item);
                        ih.path().into()
                    })
                    .collect::<Vec<dbus::Path>>())
            })
            .map_err(|e| {
                error!("Error getting items for collectioni {}: {}", self.uuid, e);
                dbus::MethodErr::failed(&format!("Error getting items for collection: {}", e))
            })
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid.clone(), |collection| Ok(collection.name.clone()))
            .map_err(|e| {
                error!("Error retrieving collectioni {}: {}", self.uuid, e);
                dbus::MethodErr::failed(&format!("Error getting label for collection: {}", e))
            })
    }
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .modify_collection(&self.uuid, |collection| {
                collection.name = value;
                Ok(())
            })
            .map_err(|e| {
                dbus::MethodErr::failed(&format!("Error setting label for collection: {}", e))
            })
    }

    fn locked(&self) -> Result<bool, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid, |collection| Ok(collection.locked))
            .map_err(|e| {
                dbus::MethodErr::failed(&format!(
                    "Error getting locked status for collection: {}",
                    e
                ))
            })
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid.clone(), |collection| Ok(collection.created))
            .map_err(|e| {
                dbus::MethodErr::failed(&format!(
                    "Error getting created timestamp for collection: {}",
                    e
                ))
            })
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_collection(self.uuid.clone(), |collection| Ok(collection.modified))
            .map_err(|e| {
                dbus::MethodErr::failed(&format!(
                    "Error getting modified timestamp for collection: {}",
                    e
                ))
            })
    }
}

impl CollectionHandle {
    fn create_item(
        collection_uuid: Uuid,
        secret: (dbus::Path, Vec<u8>, Vec<u8>, String),
        replace: bool,
        item_label: String,
        item_attributes: HashMap<String, String>,
        session_id: usize,
    ) -> Result<(dbus::Path, dbus::Path), std::io::Error> {
        let sm = SESSION_MANAGER.lock().unwrap();
        let session = sm.sessions.get(session_id).ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::Other,
                format!("Session {} not found", session_id),
            )
        })?;
        let mut storage = STORAGE.lock().map_err(|e| {
            std::io::Error::new(
                ErrorKind::Other,
                format!("Error locking storage: {}", e.to_string()),
            )
        })?;
        storage
            .with_collection(collection_uuid, |collection| {
                collection.create_item(
                    &item_label,
                    item_attributes,
                    (session, secret.1, secret.2, secret.3),
                    replace,
                )
            })
            .and_then(|item_id| {
                debug!("Item created: {}", item_id.uuid);
                let item_path = ItemHandle::from(&item_id).path();
                let item_path_clone = item_path.clone();
                tokio::spawn(async move {
                    debug!("Sending ItemCreated signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemCreated {
                            item: item_path_clone.clone().into(),
                        }
                        .to_emit_message(&item_path_clone.into()),
                    );
                });
                Ok((item_path.into(), dbus::Path::from("/")))
            })
    }

    pub fn collections() -> Result<Vec<CollectionHandle>, std::io::Error> {
        Ok(COLLECTION_HANDLES
            .lock()
            .unwrap()
            .values()
            .map(|h| h.clone())
            .collect())
    }
}
