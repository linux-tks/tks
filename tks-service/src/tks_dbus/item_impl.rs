// Purpose: Provides an implementation of the DBus interface for a secret item.
use crate::register_object;
use crate::storage::Item;
use crate::storage::ItemId;
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemChanged;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemDeleted;
use crate::tks_dbus::fdo::item::register_org_freedesktop_secret_item;
use crate::tks_dbus::fdo::item::OrgFreedesktopSecretItem;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::DBusHandlePath::SinglePath;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::MESSAGE_SENDER;
use crate::tks_dbus::{sanitize_string, DBusHandlePath};
use dbus::message::SignalArgs;
use lazy_static::lazy_static;
use log::error;
use log::{debug, trace};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ItemHandle {
    item_id: ItemId,
    path: dbus::Path<'static>,
}

pub struct ItemImpl<'a> {
    handle: &'a ItemHandle,
}

impl ItemImpl<'_> {
    pub fn new(handle: &ItemHandle) -> ItemImpl {
        ItemImpl { handle }
    }
    pub fn get_dbus_handle(&self) -> ItemHandle {
        (*self.handle).clone()
    }
}

lazy_static! {
    pub static ref ITEM_HANDLES: Arc<Mutex<HashMap<Uuid, ItemHandle>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

impl ItemHandle {
    fn new(item_id: &ItemId) -> Self {
        let handle = ItemHandle {
            path: format!(
                "/org/freedesktop/secrets/collection/{}/{}",
                sanitize_string(&item_id.collection_uuid.to_string()),
                sanitize_string(&item_id.uuid.to_string())
            )
            .to_string()
            .into(),
            item_id: item_id.clone(),
        };
        let handle_clone = handle.clone();
        register_object!(register_org_freedesktop_secret_item, handle_clone);
        handle
    }
    pub fn uuid_to_path(uuid: &Uuid) -> dbus::Path<'static> {
        ITEM_HANDLES.lock().unwrap().get(uuid).unwrap().path.clone()
    }
}

impl From<&Item> for ItemHandle {
    fn from(item: &Item) -> Self {
        ItemHandle::from(&item.id)
    }
}

impl From<&ItemId> for ItemHandle {
    fn from(item_id: &ItemId) -> Self {
        let is_new = !ITEM_HANDLES.lock().unwrap().contains_key(&item_id.uuid);
        is_new.then(|| {
            let item_handle = ItemHandle::new(&item_id);
            ITEM_HANDLES
                .lock()
                .unwrap()
                .insert(item_id.uuid, item_handle);
        });
        ITEM_HANDLES
            .lock()
            .unwrap()
            .get(&item_id.uuid)
            .unwrap()
            .clone()
    }
}

impl From<&ItemImpl<'_>> for ItemHandle {
    fn from(item_impl: &ItemImpl) -> Self {
        (*item_impl.handle).clone()
    }
}

impl DBusHandle for ItemHandle {
    fn path(&self) -> DBusHandlePath {
        SinglePath(self.path.clone())
    }
}

impl From<ItemHandle> for dbus::Path<'static> {
    fn from(handle: ItemHandle) -> Self {
        handle.path().into()
    }
}

impl OrgFreedesktopSecretItem for ItemHandle {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        match STORAGE
            .lock()
            .unwrap()
            .with_collection(self.item_id.collection_uuid, |collection| {
                collection.delete_item(&self.item_id.uuid)
            }) {
            Ok(_) => {
                let uuid: Uuid = self.item_id.uuid;
                let path: dbus::Path = self.path().clone().into();
                tokio::spawn(async move {
                    trace!("Unregistering Item");
                    ITEM_HANDLES.lock().unwrap().remove(&uuid);
                    CROSSROADS.lock().unwrap().remove::<ItemHandle>(&path);
                });
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemDeleted signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemDeleted {
                            item: item_path_clone.clone().into(),
                        }
                        .to_emit_message(&item_path_clone.into()),
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
        if self.locked()? {
            return Err(dbus::MethodErr::failed(&"Item is locked"));
        }
        let session_id = session
            .split('/')
            .last()
            .unwrap()
            .parse::<usize>()
            .map_err(|_| {
                error!("Invalid session ID");
                dbus::MethodErr::failed(&"Invalid session ID")
            })?;
        let sm = SESSION_MANAGER.lock().unwrap();
        let s = sm.sessions.get(session_id).ok_or_else(|| {
            error!("Session {} not found", session_id);
            dbus::MethodErr::failed(&"Session not found")
        })?;
        STORAGE
            .lock()
            .unwrap()
            .with_item(&self.item_id.collection_uuid, &self.item_id.uuid, |item| {
                let s = item.get_secret(s)?;
                Ok((session, s.1, s.2, s.3.clone()))
            })
            .map_err(|_| dbus::MethodErr::failed(&"Item not found"))
    }
    fn set_secret(
        &mut self,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
    ) -> Result<(), dbus::MethodErr> {
        let session_id = secret
            .0
            .split('/')
            .last()
            .unwrap()
            .parse::<usize>()
            .map_err(|_| dbus::MethodErr::failed(&"Invalid session ID"))?;

        if self
            .locked()
            .map_err(|_| dbus::MethodErr::failed(&"Item not found"))?
        {
            return Err(dbus::MethodErr::failed(&"Item is locked"));
        }

        let sm = SESSION_MANAGER.lock().unwrap();
        let s = sm.sessions.get(session_id).ok_or_else(|| {
            error!("Session {} not found", session_id);
            dbus::MethodErr::failed(&"Session not found")
        })?;

        match STORAGE.lock().unwrap().modify_item(
            &self.item_id.collection_uuid,
            &self.item_id.uuid,
            |item| item.set_secret(&s, secret.1, &secret.2, secret.3),
        ) {
            Ok(_) => {
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemChanged signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemChanged {
                            item: item_path_clone.clone().into(),
                        }
                        .to_emit_message(&item_path_clone.into()),
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
            .find(|c| c.uuid == self.item_id.collection_uuid)
            .ok_or_else(|| dbus::MethodErr::failed("Item not found"))?
            .locked;
        Ok(b)
    }
    fn attributes(&self) -> Result<::std::collections::HashMap<String, String>, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            &self.item_id.collection_uuid,
            &self.item_id.uuid,
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
        STORAGE
            .lock()
            .unwrap()
            .modify_item(
                &self.item_id.collection_uuid,
                &self.item_id.uuid,
                |item| {
                    item.attributes = value;
                    Ok(())
                },
            )
            .and_then(|_|{
                let item_path_clone = self.path().clone();
                tokio::spawn(async move {
                    debug!("Sending ItemChanged signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretCollectionItemChanged {
                            item: item_path_clone.clone().into(),
                        }
                        .to_emit_message(&item_path_clone.into()),
                    );
                });
                Ok(())
            })
            .map_err(|_| dbus::MethodErr::failed(&"Item not found"))
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        STORAGE
            .lock()
            .unwrap()
            .with_item(&self.item_id.collection_uuid, &self.item_id.uuid, |item| {
                Ok(item.label.clone())
            })
            .map_err(|_| dbus::MethodErr::failed(&"Item not found"))
    }

    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        match STORAGE.lock().unwrap().modify_item(
            &self.item_id.collection_uuid,
            &self.item_id.uuid,
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
                            item: item_path_clone.clone().into(),
                        }
                        .to_emit_message(&item_path_clone.into()),
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
                &self.item_id.collection_uuid,
                &self.item_id.uuid,
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
                &self.item_id.collection_uuid,
                &self.item_id.uuid,
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
                                item: item_path_clone.clone().into(),
                            }
                            .to_emit_message(&item_path_clone.into())
                            .into(),
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
            &self.item_id.collection_uuid,
            &self.item_id.uuid,
            |item| Ok(item.created),
        ) {
            Ok(created) => Ok(created),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            &self.item_id.collection_uuid,
            &self.item_id.uuid,
            |item| Ok(item.modified),
        ) {
            Ok(modified) => Ok(modified),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
}
