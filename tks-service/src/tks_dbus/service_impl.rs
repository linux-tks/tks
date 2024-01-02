use crate::storage::STORAGE;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretService;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretServiceCollectionCreated;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus::message::SignalArgs;
use log;
use log::{debug, error, trace};
use std::collections::HashMap;
extern crate pretty_env_logger;
use crate::convert_prop_map;
use crate::register_object;
use crate::tks_dbus::collection_impl::{CollectionHandle, CollectionImpl};
use crate::tks_dbus::fdo::collection::register_org_freedesktop_secret_collection;
use crate::tks_dbus::fdo::session::register_org_freedesktop_secret_session;
use crate::tks_dbus::session_impl::SessionHandle;
use crate::tks_dbus::CROSSROADS;

use dbus::arg;

pub struct ServiceHandle {}
pub struct ServiceImpl {}

impl ServiceImpl {
    pub fn new() -> ServiceImpl {
        let coll = CollectionImpl::new("default");
        tokio::spawn(async move {
            trace!("Registering default collection");
            register_object!(
                register_org_freedesktop_secret_collection::<CollectionHandle>,
                coll.get_dbus_handle()
            );
        });
        ServiceImpl {}
    }
    pub fn get_dbus_handle(&self) -> ServiceHandle {
        ServiceHandle {}
    }
}
impl DBusHandle for ServiceHandle {
    fn path(&self) -> dbus::Path<'static> {
        "/org/freedesktop/secrets".to_string().into()
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
        trace!("open_session {}", algorithm);
        let mut sm = SESSION_MANAGER.lock().unwrap();
        match sm.new_session(algorithm, arg::cast(&input.0)) {
            Ok((sess_id, vector)) => {
                let output = match vector {
                    Some(e) => arg::Variant(Box::new(e.clone()) as Box<dyn arg::RefArg + 'static>),
                    None => arg::Variant(Box::new(String::new()) as Box<dyn arg::RefArg + 'static>),
                };
                let path = {
                    let dh = sm.sessions.get(sess_id).unwrap().get_dbus_handle();
                    let path = dh.path();
                    register_object!(register_org_freedesktop_secret_session::<SessionHandle>, dh);
                    path
                };
                Ok((output, path))
            }
            Err(e) => {
                error!("Error creating session: {}", e);
                Err(dbus::MethodErr::failed(&format!(
                    "Error creating session: {}",
                    e
                )))
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
        trace!("create_collection alias={}", alias);

        match alias.as_str() {
            "default" => {
                let prompt_path = dbus::Path::from("/");
                // TODO: should we emit the CollectionCreated signal here?
                return Ok((
                    dbus::Path::from("/org/freedesktop/secrets/collection/default"),
                    prompt_path,
                ));
            }
            _ => {}
        }
        let (string_props, _) = convert_prop_map!(properties);

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
            .create_collection(&label, &alias, &string_props)
        {
            Ok(_) => {
                let coll = CollectionImpl::new(&label);
                let collection_path = coll.get_dbus_handle().path();
                register_object!(
                    register_org_freedesktop_secret_collection::<CollectionHandle>,
                    coll.get_dbus_handle()
                );
                let collection_path_clone = collection_path.clone();
                tokio::spawn(async move {
                    debug!("Sending CollectionCreated signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretServiceCollectionCreated {
                            collection: collection_path_clone.clone(),
                        }
                        .to_emit_message(&collection_path_clone),
                    );
                });
                let prompt_path = dbus::Path::from("/");
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
        let mut unlocked = Vec::new();
        let mut locked = Vec::new();

        macro_rules! collect_paths {
            ($locked:ident, $vec:ident) => {
                STORAGE
                    .lock()
                    .unwrap()
                    .collections
                    .iter()
                    .filter(|c| c.locked == $locked)
                    .filter(|c| c.items.is_some())
                    .for_each(|c| {
                        $vec.extend(
                            c.items
                                .as_ref()
                                .unwrap()
                                .iter()
                                .filter(|i| i.attributes == attributes)
                                .map(|i| {
                                    dbus::Path::from(format!(
                                        "/org/freedesktop/secrets/collection/{}/{}",
                                        c.name, i.label
                                    ))
                                }),
                        );
                    })
            };
        }
        collect_paths!(true, locked);
        collect_paths!(false, unlocked);
        Ok((unlocked, locked))
    }
    fn unlock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("unlock {:?}", objects);
        let collection_names = objects
            .iter()
            .map(|p| p.to_string())
            .map(|p| p.split('/').map(|s| s.to_string()).collect::<Vec<String>>()[5].clone())
            .collect::<Vec<String>>();
        let mut unlocked = Vec::new();
        STORAGE
            .lock()
            .unwrap()
            .collections
            .iter_mut()
            .filter(|c| collection_names.contains(&c.name))
            .for_each(|c| {
                match c.unlock() {
                    Ok(_) => {
                        objects.iter().for_each(|p| {
                            if p.to_string().contains(&c.name) {
                                unlocked.push(p.clone());
                            }
                        });
                    }
                    Err(e) => {
                        // TODO this may instead require a prompt
                        assert!(false, "Error unlocking collection: {}", e);
                    }
                }
            });
        Ok((unlocked, dbus::Path::from("/")))
    }
    fn lock(
        &mut self,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("lock {:?}", objects);
        let collection_names = objects
            .iter()
            .map(|p| p.to_string())
            .map(|p| p.split('/').map(|s| s.to_string()).collect::<Vec<String>>()[5].clone())
            .collect::<Vec<String>>();
        let mut locked: Vec<dbus::Path> = Vec::new();
        STORAGE
            .lock()
            .unwrap()
            .collections
            .iter_mut()
            .filter(|c| collection_names.contains(&c.name))
            .for_each(|c| {
                if c.lock().unwrap_or(false) {
                    let path = dbus::Path::from(format!(
                        "/org/freedesktop/secrets/collection/{}",
                        c.name.clone()
                    ));
                    locked.push(path);
                }
            });
        Ok((locked, dbus::Path::from("/")))
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
        trace!("get_secrets {:?}", items);
        let session_id = match session.split('/').last().unwrap().parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                error!("Invalid session ID");
                return Err(dbus::MethodErr::failed(&"Invalid session ID"));
            }
        };
        let sm = SESSION_MANAGER.lock().unwrap();
        let session = match sm.sessions.get(session_id) {
            Some(s) => s,
            None => {
                error!("Session {} not found", session_id);
                return Err(dbus::MethodErr::failed(&"Session not found"));
            }
        };
        type Secret = (dbus::Path<'static>, Vec<u8>, Vec<u8>, String);
        let mut secrets_map: HashMap<dbus::Path, Secret> = HashMap::new();
        items
            .iter()
            .map(|p| {
                (
                    p,
                    p.to_string()
                        .split('/')
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                )
            })
            .filter(|p| p.1.len() == 7)
            .for_each(|p| {
                let coll = p.1.get(5).unwrap().clone();
                let item = p.1.get(6).unwrap().clone();
                let _ = STORAGE
                    .lock()
                    .unwrap()
                    .with_item(&coll, &item, |i| i.get_secret(session))
                    .map(|s| {
                        secrets_map.insert(p.0.clone(), (dbus::Path::from(s.0), s.1, s.2, s.3));
                    });
            });
        Ok(secrets_map)
    }

    fn read_alias(&mut self, name: String) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        trace!("read_alias {}", name);
        match name.as_str() {
            "default" => match STORAGE.lock().unwrap().read_alias(&name) {
                Ok(name) => Ok(dbus::Path::from(format!(
                    "/org/freedesktop/secrets/collection/{}",
                    name
                ))),
                Err(e) => {
                    error!("Error reading alias: {}", e);
                    return Err(dbus::MethodErr::failed(&format!(
                        "Error reading alias: {}",
                        e
                    )));
                }
            },
            x => {
                return Err(dbus::MethodErr::failed(&format!(
                    "Alias not recognized: '{}'",
                    x
                )));
            }
        }
    }
    fn set_alias(
        &mut self,
        _name: String,
        _collection: dbus::Path<'static>,
    ) -> Result<(), dbus::MethodErr> {
        trace!("Hello from set_alias");
        return Err(dbus::MethodErr::failed(&format!(
            "Error setting alias: {}",
            "Not implemented"
        )));
    }
    fn collections(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        trace!("collections");
        let collections = &STORAGE.lock().unwrap().collections;
        let c = collections
            .into_iter()
            .map(|c| dbus::Path::from(format!("/org/freedesktop/secrets/collection/{}", c.name)))
            .collect();
        Ok(c)
    }
}
