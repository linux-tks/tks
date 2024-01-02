// Purpose: Provides an implementation of the DBus interface for a secret item.
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemChanged;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemDeleted;
use crate::tks_dbus::fdo::item::OrgFreedesktopSecretItem;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus::message::SignalArgs;
use log::debug;
use log::error;

pub struct ItemHandle {
    label: String,
    collection_alias: String,
}

pub struct ItemImpl {
    label: String,
    collection_alias: String,
}

impl ItemImpl {
    pub fn new(label: &str, collection_alias: &str) -> ItemImpl {
        ItemImpl {
            label: label.to_string(),
            collection_alias: collection_alias.to_string(),
        }
    }
    pub fn get_dbus_handle(&self) -> ItemHandle {
        ItemHandle {
            label: self.label.clone(),
            collection_alias: self.collection_alias.clone(),
        }
    }
}

impl DBusHandle for ItemHandle {
    fn path(&self) -> dbus::Path<'static> {
        format!(
            "/org/freedesktop/secrets/collection/{}/{}",
            self.collection_alias, self.label
        )
        .to_string()
        .into()
    }
}

impl OrgFreedesktopSecretItem for ItemHandle {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(self.collection_alias.as_str(), |collection| {
                collection.delete_item(&self.label)
            }) {
            Ok(_) => {
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemDeleted signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemDeleted {
                            item: item_path_clone.clone(),
                        }
                        .to_emit_message(&item_path_clone),
                    );
                });
                let prompt_path = dbus::Path::from("/");
                Ok(prompt_path)
            }
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn get_secret(
        &mut self,
        session: dbus::Path<'static>,
    ) -> Result<(dbus::Path<'static>, Vec<u8>, Vec<u8>, String), dbus::MethodErr> {
        let session_id = match session.split('/').last().unwrap().parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                error!("Invalid session ID");
                return Err(dbus::MethodErr::failed(&"Invalid session ID"));
            }
        };
        match self.locked() {
            Ok(true) => return Err(dbus::MethodErr::failed(&"Item is locked")),
            Err(_) => return Err(dbus::MethodErr::failed(&"Item not found")),
            Ok(false) => {}
        }
        let sm = SESSION_MANAGER.lock().unwrap();
        let s = match sm.sessions.get(session_id) {
            Some(s) => s,
            None => {
                error!("Session {} not found", session_id);
                return Err(dbus::MethodErr::failed(&"Session not found"));
            }
        };
        match self.locked() {
            Ok(true) => Err(dbus::MethodErr::failed(&"Item is locked")),
            Ok(false) => match STORAGE.lock().unwrap().with_item(
                self.collection_alias.as_str(),
                self.label.as_str(),
                |item| {
                    let s = item.get_secret(s)?;
                    Ok((dbus::Path::from(s.0), s.1, s.2, s.3.clone()))
                },
            ) {
                Ok(result) => Ok(result),
                Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
            },
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn set_secret(
        &mut self,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
    ) -> Result<(), dbus::MethodErr> {
        let session_id = match secret.0.split('/').last().unwrap().parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                error!("Invalid session ID");
                return Err(dbus::MethodErr::failed(&"Invalid session ID"));
            }
        };
        match self.locked() {
            Ok(true) => return Err(dbus::MethodErr::failed(&"Item is locked")),
            Err(_) => return Err(dbus::MethodErr::failed(&"Item not found")),
            Ok(false) => {}
        }
        let sm = SESSION_MANAGER.lock().unwrap();
        let s = match sm.sessions.get(session_id) {
            Some(s) => s,
            None => {
                error!("Session {} not found", session_id);
                return Err(dbus::MethodErr::failed(&"Session not found"));
            }
        };
        match STORAGE.lock().unwrap().modify_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| item.set_secret(&s, secret.1, &secret.2, secret.3),
        ) {
            Ok(_) => {
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemChanged signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemChanged {
                            item: item_path_clone.clone(),
                        }
                        .to_emit_message(&item_path_clone),
                    );
                });
                Ok(())
            }
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn locked(&self) -> Result<bool, dbus::MethodErr> {
        let b = STORAGE
            .lock()
            .unwrap()
            .collections
            .iter()
            .find(|c| c.name == self.collection_alias)
            .unwrap()
            .locked;
        Ok(b)
    }
    fn attributes(&self) -> Result<::std::collections::HashMap<String, String>, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| Ok(item.attributes.clone()),
        ) {
            Ok(attrs) => Ok(attrs),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn set_attributes(
        &self,
        value: ::std::collections::HashMap<String, String>,
    ) -> Result<(), dbus::MethodErr> {
        match STORAGE.lock().unwrap().modify_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| {
                item.attributes = value;
                Ok(())
            },
        ) {
            Ok(_) => {
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemChanged signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemChanged {
                            item: item_path_clone.clone(),
                        }
                        .to_emit_message(&item_path_clone),
                    );
                });
                Ok(())
            }
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.label.clone())
    }

    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        match STORAGE.lock().unwrap().modify_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| {
                item.label = value;
                Ok(())
            },
        ) {
            Ok(_) => {
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemChanged signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemChanged {
                            item: item_path_clone.clone(),
                        }
                        .to_emit_message(&item_path_clone),
                    );
                });
                Ok(())
            }
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }

    fn type_(&self) -> Result<String, dbus::MethodErr> {
        match self.locked() {
            Ok(true) => Err(dbus::MethodErr::failed(&"Item is locked")),
            Ok(false) => match STORAGE.lock().unwrap().with_item(
                self.collection_alias.as_str(),
                self.label.as_str(),
                |item| match item.data {
                    Some(ref data) => Ok(data.content_type.clone()),
                    None => Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Not found"),
                    )),
                },
            ) {
                Ok(content_type) => Ok(content_type),
                Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
            },
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }

    fn set_type(&self, value: String) -> Result<(), dbus::MethodErr> {
        match self.locked() {
            Ok(true) => Err(dbus::MethodErr::failed(&"Item is locked")),
            Ok(false) => match STORAGE.lock().unwrap().modify_item(
                self.collection_alias.as_str(),
                self.label.as_str(),
                |item| {
                    item.data.as_mut().unwrap().content_type = value;
                    Ok(())
                },
            ) {
                Ok(_) => {
                    let item_path_clone = self.path().clone();
                    tokio::spawn(async move {
                        debug!("Sending ItemChanged signal");
                        MESSAGE_SENDER.lock().unwrap().send_message(
                            OrgFreedesktopSecretCollectionItemChanged {
                                item: item_path_clone.clone(),
                            }
                            .to_emit_message(&item_path_clone),
                        );
                    });
                    Ok(())
                }
                Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
            },
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| Ok(item.created),
        ) {
            Ok(created) => Ok(created),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| Ok(item.modified),
        ) {
            Ok(modified) => Ok(modified),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
}
