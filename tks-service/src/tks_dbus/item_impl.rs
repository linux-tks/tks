// Purpose: Provides an implementation of the DBus interface for a secret item.
use crate::storage::STORAGE;
use crate::tks_dbus::fdo::item::OrgFreedesktopSecretItem;
use crate::tks_dbus::DBusHandle;
use dbus::arg;
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
        let session = session.to_string();
        match self.locked() {
            Ok(true) => Err(dbus::MethodErr::failed(&"Item is locked")),
            Ok(false) => match STORAGE.lock().unwrap().with_item(
                self.collection_alias.as_str(),
                self.label.as_str(),
                |item| match item.get_secret(&session) {
                    Ok((session, parameters, value, content_type)) => {
                        Ok((dbus::Path::from(session), parameters, value, content_type))
                    }
                    Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
                },
            ) {
                Ok(result) => result,
                Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
            },
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn set_secret(
        &mut self,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
    ) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
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
            |item| item.attributes.clone(),
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
            Ok(_) => Ok(()),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.label.clone())
    }
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        error!("Setting label to {}", value);
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn type_(&self) -> Result<String, dbus::MethodErr> {
        error!("Getting type");
        Err(dbus::MethodErr::failed(&"Not implemented"))
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
                Ok(_) => Ok(()),
                Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
            },
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| item.created,
        ) {
            Ok(created) => Ok(created),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        match STORAGE.lock().unwrap().with_item(
            self.collection_alias.as_str(),
            self.label.as_str(),
            |item| item.modified,
        ) {
            Ok(modified) => Ok(modified),
            Err(_) => Err(dbus::MethodErr::failed(&"Item not found")),
        }
    }
}
