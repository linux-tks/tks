use crate::tks_dbus::fdo::service::{
    register_org_freedesktop_secret_service, OrgFreedesktopSecretService,
};
use crate::tks_dbus::session_impl::create_session;
use crate::tks_dbus::session_impl::DBusProxy;
use dbus::arg;
use dbus_crossroads as crossroads;
use log::{debug, error, info, trace};

pub struct ServiceProxy {}
pub struct ServiceImpl {}

impl ServiceImpl {
    pub fn new() -> ServiceImpl {
        ServiceImpl {}
    }
    pub fn get_proxy(&self) -> ServiceProxy {
        ServiceProxy {}
    }
}
impl DBusProxy for ServiceProxy {
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
    fn create_collection(
        &mut self,
        properties: arg::PropMap,
        alias: String,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("Hello from create_collection");
        Ok((dbus::Path::from("/"), dbus::Path::from("/")))
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<(Vec<dbus::Path<'static>>, Vec<dbus::Path<'static>>), dbus::MethodErr> {
        trace!("Hello fromi search_items");
        Ok((vec![], vec![]))
    }
    fn unlock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("Hello fromi unlock");
        Ok((vec![], dbus::Path::from("/")))
    }
    fn lock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("Hello from lock");
        Ok((vec![], dbus::Path::from("/")))
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
        Ok(::std::collections::HashMap::new())
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
        Ok(())
    }
    fn collections(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        trace!("Hello from collections");
        Ok(vec![])
    }
}
