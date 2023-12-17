use crate::tks_dbus::fdo::collection::OrgFreedesktopSecretCollection;
use crate::tks_dbus::session_impl::DBusHandle;
use dbus::arg;

pub struct CollectionHandle {
    alias: String,
}
pub struct CollectionImpl {
    alias: String,
}

impl CollectionImpl {
    pub fn new() -> CollectionImpl {
        CollectionImpl {
            alias: String::new(),
        }
    }
    pub fn get_dbus_handle(&self) -> CollectionHandle {
        CollectionHandle {
            alias: self.alias.clone(),
        }
    }
}
impl DBusHandle for CollectionHandle {
    fn path(&self) -> String {
        format!("/org/freedesktop/secrets/collection/{}", self.alias).to_string()
    }
}

impl OrgFreedesktopSecretCollection for CollectionImpl {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        Err(dbus::MethodErr::failed(&"Not implemented"))
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
        Err(dbus::MethodErr::failed(&"Not implemented"))
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
