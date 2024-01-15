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
        if let Ok(paths) = ServiceImpl::collections_and_paths() {
            paths.iter().for_each(|p| {
                let coll_handle = CollectionHandle {
                    alias: p.0.clone(),
                    path: Some(p.1.clone()),
                };
                register_object!(
                    register_org_freedesktop_secret_collection::<CollectionHandle>,
                    coll_handle
                );
            });
        } else {
            error!("Error received while attempting to read get stored collections. Service may not by able to operate.");
        }
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
        sm.new_session(algorithm, arg::cast(&input.0))
            .or_else(|e| {
                error!("Error creating session: {}", e);
                Err(dbus::MethodErr::failed(&format!(
                    "Error creating session: {}",
                    e
                )))
            })
            .and_then(|(sess_id, vector)| {
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
            })
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
                // TODO: should we emit the CollectionCreated signal here?
                return Ok((
                    dbus::Path::from("/org/freedesktop/secrets/collection/default"),
                    dbus::Path::from("/"),
                ));
            }
            _ => {}
        }
        let (string_props, _) = convert_prop_map!(properties);

        // now check if user specified the org.freedesktop.Secret.Collection.Label property
        let label = string_props
            .get("org.freedesktop.Secret.Collection.Label")
            .ok_or_else(|| {
                dbus::MethodErr::failed(&format!(
                    "Error creating collection: {}",
                    "No label specified"
                ))
            })?;

        STORAGE
            .lock()
            .unwrap()
            .create_collection(&label, &alias, &string_props)
            .and_then(|()| {
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
                Ok((collection_path, prompt_path))
            })
            .map_err(|e| {
                error!("Error creating collection: {}", e);
                dbus::MethodErr::failed(&format!("Error creating collection: {}", e))
            })
    }
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<(Vec<dbus::Path<'static>>, Vec<dbus::Path<'static>>), dbus::MethodErr> {
        trace!("search_items {:?}", attributes);
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
        debug!("search_items unlocked: {:?}", unlocked);
        debug!("search_items locked: {:?}", locked);
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
                let _ = c
                    .unlock()
                    .and_then(|()| {
                        objects.iter().for_each(|p| {
                            if p.to_string().contains(&c.name) {
                                unlocked.push(p.clone());
                            }
                        });
                        Ok(())
                    })
                    .map_err(|e| {
                        // TODO this may instead require a prompt
                        error!("Couldn't lock collection {}: {:?}", c.name, e);
                    });
            });
        let prompt = dbus::Path::from("/");
        debug!("unlocked: {:?}, prompt: {:?}", unlocked, prompt);
        Ok((unlocked, prompt))
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
        let session = sm.sessions.get(session_id).ok_or_else(|| {
            error!("Session {} not found", session_id);
            dbus::MethodErr::failed(&"Session not found")
        })?;
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
            "default" => STORAGE
                .lock()
                .unwrap()
                .read_alias(&name)
                .map(|name| {
                    dbus::Path::from(format!("/org/freedesktop/secrets/collection/{}", name))
                })
                .map_err(|e| {
                    error!("Error reading alias: {}", e);
                    dbus::MethodErr::failed(&format!("Error reading alias: {}", e))
                }),
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
        let (_, paths): (Vec<String>, Vec<dbus::Path>) = ServiceImpl::collections_and_paths()
            .map_err(|e| {
                error!("Error getting collections: {}", e);
                dbus::MethodErr::failed(&format!("Error getting collections: {}", e))
            })?
            .iter()
            .cloned()
            .unzip();
        Ok(paths)
    }
}

impl ServiceImpl {
    fn collections_and_paths() -> Result<Vec<(String, dbus::Path<'static>)>, std::io::Error> {
        let collections = &STORAGE
            .lock()
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting settings: {}", e),
                )
            })?
            .collections;
        Ok(collections
            .into_iter()
            .flat_map(|c| {
                let mut paths: Vec<(String, dbus::Path)> = Vec::new();
                let empty: Vec<String> = Vec::new();
                paths.push((
                    c.name.clone(),
                    dbus::Path::from(format!("/org/freedesktop/secrets/collection/{}", c.name)),
                ));
                paths.extend(
                    c.aliases
                        .as_ref()
                        .unwrap_or_else(|| &empty)
                        .iter()
                        .map(|s| {
                            (
                                c.name.clone(),
                                dbus::Path::from(format!("/org/freedesktop/secrets/aliases/{}", s)),
                            )
                        }),
                );
                paths
            })
            .collect())
    }
}
