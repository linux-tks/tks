// Purpose: Provides an implementation of the DBus interface for a secret item.
use crate::tks_dbus::fdo::item::OrgFreedesktopSecretItem;
use crate::tks_dbus::DBusHandle;

pub struct ItemHandle {
    alias: String,
}

pub struct ItemImpl {
    alias: String,
}

impl ItemImpl {
    pub fn new(alias: &str) -> ItemImpl {
        ItemImpl {
            alias: alias.to_string(),
        }
    }
    pub fn get_dbus_handle(&self) -> ItemHandle {
        ItemHandle {
            alias: self.alias.clone(),
        }
    }
}

impl DBusHandle for ItemHandle {
    fn path(&self) -> dbus::Path<'static> {
        format!("/org/freedesktop/secrets/item/{}", self.alias)
            .to_string()
            .into()
    }
}

impl OrgFreedesktopSecretItem for ItemHandle {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn get_secret(
        &mut self,
        session: dbus::Path<'static>,
    ) -> Result<(dbus::Path<'static>, Vec<u8>, Vec<u8>, String), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn set_secret(
        &mut self,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
    ) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn locked(&self) -> Result<bool, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn attributes(&self) -> Result<::std::collections::HashMap<String, String>, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn set_attributes(
        &self,
        value: ::std::collections::HashMap<String, String>,
    ) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn label(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn type_(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn set_type(&self, value: String) -> Result<(), dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn created(&self) -> Result<u64, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
    fn modified(&self) -> Result<u64, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
    }
}
