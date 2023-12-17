use crate::storage::STORAGE;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretService;
use crate::tks_dbus::session_impl::create_session;
use crate::tks_dbus::session_impl::DBusHandle;
use log;
use log::{debug, error, info, trace};
use std::collections::HashMap;
extern crate pretty_env_logger;
use dbus::arg;

pub struct ServiceHandle {}
pub struct ServiceImpl {}

impl ServiceImpl {
    pub fn new() -> ServiceImpl {
        ServiceImpl {}
    }
    pub fn get_dbus_handle(&self) -> ServiceHandle {
        ServiceHandle {}
    }
}
impl DBusHandle for ServiceHandle {
    fn path(&self) -> String {
        "/org/freedesktop/secrets".to_string()
    }
}

impl OrgFreedesktopSecretService for ServiceImpl {
    fn open_session(
        &mut self,
        algorithm: String,
        input: arg::Variant<Box<dyn arg::RefArg + 'static>>,
    ) -> Result<
        (
            arg::Variant<Box<dyn arg::RefArg + 'static>>,
            dbus::Path<'static>,
        ),
        dbus::MethodErr,
    > {
        debug!("open_session {}", algorithm);
        match create_session(algorithm, arg::cast::<Vec<u8>>(&input.0)) {
            Ok((path, vector)) => {
                let path = dbus::Path::from(path);
                let output = match vector {
                    Some(e) => arg::Variant(Box::new(e) as Box<dyn arg::RefArg>),
                    None => arg::Variant(Box::new(String::new()) as Box<dyn arg::RefArg>),
                };
                Ok((output, path))
            }
            Err(e) => {
                error!("Error creating session: {}", e);
                return Err(dbus::MethodErr::failed(&format!(
                    "Error creating session: {}",
                    e
                )));
            }
        }
    }

    /// Create a new collection
    /// # Arguments
    /// * `properties` - A HashMap of properties to set on the collection; this version ignores any
    /// properties but the org.freedesktop.Secret.Collection.Label property, which is required
    /// * `alias` - The alias to use for the collection
    fn create_collection(
        &mut self,
        properties: arg::PropMap,
        alias: String,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        debug!("create_collection {}", alias);
        // the DBus spec says that properties is a dict of string:variant, but really it should be
        // a dict of string:String
        let mut errors: Vec<String> = Vec::new();
        let string_props: HashMap<String, String> = properties
            .iter()
            .map(|(k, v)| match arg::cast::<String>(&v.0) {
                Some(s) => (k.clone(), s.clone()),
                None => {
                    debug!("Error casting property {} to string", k);
                    errors.push(format!("Property {} should be a string", k));
                    (k.clone(), String::new())
                }
            })
            .collect();

        if errors.len() > 0 {
            return Err(dbus::MethodErr::failed(&format!(
                "Error creating collection: {}",
                errors.join(", ")
            )));
        }

        // now check if user specified the org.freedesktop.Secret.Collection.Label property
        let label = match string_props.get("org.freedesktop.Secret.Collection.Label") {
            Some(s) => s.clone(),
            None => {
                debug!("Error creating collection: no label specified");
                return Err(dbus::MethodErr::failed(&format!(
                    "Error creating collection: {}",
                    "No label specified"
                )));
            }
        };

        match STORAGE
            .lock()
            .unwrap()
            .create_collection(&label, &string_props)
        {
            Ok(_) => {
                let prompt_path = dbus::Path::from("/");
                let collection_path =
                    dbus::Path::from(format!("/org/freedesktop/secrets/collection/{}", label));
                return Ok((collection_path, prompt_path));
            }
            Err(e) => {
                error!("Error creating collection: {}", e);
                return Err(dbus::MethodErr::failed(&format!(
                    "Error creating collection: {}",
                    e
                )));
            }
        }
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<(Vec<dbus::Path<'static>>, Vec<dbus::Path<'static>>), dbus::MethodErr> {
        trace!("Hello from search_items");
        // Ok((vec![], vec![]))
        return Err(dbus::MethodErr::failed(&format!(
            "Error searching items: {}",
            "Not implemented"
        )));
    }
    fn unlock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("Hello from unlock");
        // Ok((vec![], dbus::Path::from("/")))
        return Err(dbus::MethodErr::failed(&format!(
            "Error unlocking items: {}",
            "Not implemented"
        )));
    }
    fn lock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("Hello from lock");
        // Ok((vec![], dbus::Path::from("/")))
        return Err(dbus::MethodErr::failed(&format!(
            "Error locking items: {}",
            "Not implemented"
        )));
    }
    fn get_secrets(
        &mut self,
        items: Vec<dbus::Path<'static>>,
        session: dbus::Path<'static>,
    ) -> Result<
        ::std::collections::HashMap<
            dbus::Path<'static>,
            (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        >,
        dbus::MethodErr,
    > {
        trace!("Hello from get_secrets");
        // Ok(::std::collections::HashMap::new())
        return Err(dbus::MethodErr::failed(&format!(
            "Error getting secrets: {}",
            "Not implemented"
        )));
    }
    fn read_alias(&mut self, name: String) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        trace!("Hello from read_alias");
        Ok(dbus::Path::from("/"))
    }
    fn set_alias(
        &mut self,
        name: String,
        collection: dbus::Path<'static>,
    ) -> Result<(), dbus::MethodErr> {
        trace!("Hello from set_alias");
        // Ok(())
        return Err(dbus::MethodErr::failed(&format!(
            "Error setting alias: {}",
            "Not implemented"
        )));
    }
    fn collections(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        trace!("Hello from collections");
        // Ok(vec![])
        return Err(dbus::MethodErr::failed(&format!(
            "Error getting collections: {}",
            "Not implemented"
        )));
    }
}
