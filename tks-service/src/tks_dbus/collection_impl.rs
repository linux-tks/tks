use crate::convert_prop_map;
use crate::register_object;
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollection;
use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollectionItemCreated;
use crate::tks_dbus::fdo::item::register_org_freedesktop_secret_item;
use crate::tks_dbus::item_impl::ItemHandle;
use crate::tks_dbus::item_impl::ItemImpl;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus::arg;
use dbus::message::SignalArgs;
use log::{debug, error};
use std::collections::HashMap;

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
            _ => Err(dbus::MethodErr::failed(&"Not implemented")),
        }
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn create_item(
        &mut self,
        properties: arg::PropMap,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        replace: bool,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        let (string_props, _errors) = convert_prop_map!(properties);
        let mut storage = STORAGE.lock().unwrap();
        let rc = storage
            .with_collection(&self.alias, |collection| {
                collection.create_item(string_props, secret, replace)
            })
            .and_then(|item| item)
            .map_err(|e| {
                error!("Error creating item: {}", e);
                dbus::MethodErr::failed(&format!("Error creating item: {}", e))
            });
        match rc {
            Ok(_) => {
                let item = ItemImpl::new(&self.alias);
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
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn locked(&self) -> Result<bool, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
}
