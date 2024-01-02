use crate::register_object;
use crate::storage::Item;
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollection;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemCreated;
use crate::tks_dbus::fdo::item::register_org_freedesktop_secret_item;
use crate::tks_dbus::item_impl::ItemHandle;
use crate::tks_dbus::item_impl::ItemImpl;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus::arg;
use dbus::message::SignalArgs;
use log::{debug, error};

pub struct CollectionHandle {
    alias: String,
}
pub struct CollectionImpl {
    alias: String,
}

impl CollectionImpl {
    pub fn new(alias: &str) -> CollectionImpl {
        CollectionImpl {
            alias: alias.to_string(),
        }
    }
    pub fn get_dbus_handle(&self) -> CollectionHandle {
        CollectionHandle {
            alias: self.alias.clone(),
        }
    }
}
impl DBusHandle for CollectionHandle {
    fn path(&self) -> dbus::Path<'static> {
        format!("/org/freedesktop/secrets/collection/{}", self.alias)
            .to_string()
            .into()
    }
}

impl OrgFreedesktopSecretCollection for CollectionHandle {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        debug!("delete called on '{}'", self.alias);
        match self.alias.as_str() {
            "default" => return Err(dbus::MethodErr::failed(&"Cannot delete default collection")),
            // TODO: implement this when prompts are implemented
            _ => Err(dbus::MethodErr::failed(&"Not implemented")),
        }
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        let empty: Vec<Item> = Vec::new();
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(&self.alias, |collection| {
                collection
                    .items
                    .as_ref()
                    .unwrap_or_else(|| &empty)
                    .iter()
                    .filter(|item| item.attributes == attributes)
                    .map(|item| format!("{}/{}", self.path(), item.label))
                    .collect::<Vec<String>>()
            }) {
            Ok(items) => Ok(items.iter().map(|i| i.clone().into()).collect()),
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error searching items for collection {}: {}",
                self.alias, e
            ))),
        }
    }
    // d-feet example call:
    // {"org.freedesktop.Secret.Item.Label":GLib.Variant('s',"test"), "org.freedesktop.Secret.Item.Attributes":GLib.Variant("a{sv}",{"prop1":GLib.Variant('s',"val1"),"prop2":GLib.Variant('s',"val2")})}, ("/",[],[],""),0
    fn create_item(
        &mut self,
        properties: arg::PropMap,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        replace: bool,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        let item_label = match properties.get("org.freedesktop.Secret.Item.Label") {
            Some(s) => match arg::cast::<String>(&s.0) {
                Some(s) => s.clone(),
                None => {
                    debug!("Error creating item: label is not a string");
                    return Err(dbus::MethodErr::failed(&format!(
                        "Error creating item: {}",
                        "Label is not a string"
                    )));
                }
            },
            None => {
                debug!("Error creating item: no label specified");
                return Err(dbus::MethodErr::failed(&format!(
                    "Error creating item: {}",
                    "No label specified"
                )));
            }
        };
        let mut errors = Vec::new();
        let item_attributes = match properties.get("org.freedesktop.Secret.Item.Attributes") {
            Some(d) => match arg::cast::<arg::PropMap>(&d.0) {
                Some(d) => d
                    .iter()
                    .map(|(k, v)| match arg::cast::<String>(&v.0) {
                        Some(s) => (k.clone(), s.clone()),
                        None => {
                            debug!("Error casting property {} to string", k);
                            errors.push(format!("Property {} should be a string", k));
                            (k.clone(), String::new())
                        }
                    })
                    .collect(),
                None => {
                    debug!("Error creating item: no attributes specified");
                    return Err(dbus::MethodErr::failed(&format!(
                        "Error creating item: {}",
                        "No attributes specified"
                    )));
                }
            },

            None => {
                debug!("Error creating item: no attributes specified");
                return Err(dbus::MethodErr::failed(&format!(
                    "Error creating item: {}",
                    "No attributes specified"
                )));
            }
        };
        let session_id = match secret.0.split('/').last().unwrap().parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                error!("Invalid session ID");
                return Err(dbus::MethodErr::failed(&"Invalid session ID"));
            }
        };
        match self.locked() {
            Ok(true) => return Err(dbus::MethodErr::failed(&"Collection is locked")),
            Err(_) => return Err(dbus::MethodErr::failed(&"Not found")),
            Ok(false) => {}
        }
        let sm = SESSION_MANAGER.lock().unwrap();
        let session = match sm.sessions.get(session_id) {
            Some(s) => s,
            None => {
                error!("Session {} not found", session_id);
                return Err(dbus::MethodErr::failed(&"Session not found"));
            }
        };
        let mut storage = STORAGE.lock().unwrap();
        let rc = storage
            .with_collection(&self.alias, |collection| {
                collection.create_item(
                    &item_label,
                    item_attributes,
                    (session, secret.1, secret.2, secret.3),
                    replace,
                )
            })
            .and_then(|item| item)
            .map_err(|e| {
                error!("Error creating item: {}", e);
                dbus::MethodErr::failed(&format!("Error creating item: {}", e))
            });
        match rc {
            Ok(_) => {
                let item = ItemImpl::new(&item_label, &self.alias);
                let item_path = item.get_dbus_handle().path();
                register_object!(
                    register_org_freedesktop_secret_item::<ItemHandle>,
                    item.get_dbus_handle()
                );
                let item_path_clone = item_path.clone();
                tokio::spawn(async move {
                    debug!("Sending ItemCreated signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemCreated {
                            item: item_path_clone.clone(),
                        }
                        .to_emit_message(&item_path_clone),
                    );
                });
                let prompt_path = dbus::Path::from("/");
                Ok((item_path, prompt_path))
            }
            Err(err) => Err(dbus::MethodErr::failed(&format!(
                "Error creating item {}",
                err
            ))),
        }
    }
    fn items(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(&self.alias, |collection| match &collection.items {
                Some(items) => {
                    let is = items
                        .iter()
                        .map(|item| format!("{}/{}", self.path(), item.label))
                        .collect();
                    Some(is)
                }
                None => None,
            }) {
            Ok(items) => {
                let items = items.unwrap_or(Vec::new());
                Ok(items.into_iter().map(|i| i.into()).collect())
            }
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error getting items for collection {}: {}",
                self.alias, e
            ))),
        }
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.alias.clone())
    }
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .modify_collection(&self.alias, |collection| {
                collection.name = value;
                Ok(())
            }) {
            Ok(_) => Ok(()),
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error setting label for collection {}: {}",
                self.alias, e
            ))),
        }
    }
    fn locked(&self) -> Result<bool, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(&self.alias, |collection| collection.locked)
        {
            Ok(locked) => Ok(locked),
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error getting locked status for collection {}: {}",
                self.alias, e
            ))),
        }
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(&self.alias, |collection| collection.created)
        {
            Ok(created) => Ok(created),
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error getting created timestamp for collection {}: {}",
                self.alias, e
            ))),
        }
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(&self.alias, |collection| collection.modified)
        {
            Ok(modified) => Ok(modified),
            Err(e) => Err(dbus::MethodErr::failed(&format!(
                "Error getting modified timestamp for collection {}: {}",
                self.alias, e
            ))),
        }
    }
}
