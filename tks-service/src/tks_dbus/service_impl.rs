use crate::storage::STORAGE;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretService;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretServiceCollectionCreated;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::{sanitize_string, DBusHandle};
use crate::tks_dbus::{DBusHandlePath, MESSAGE_SENDER};
use dbus::message::SignalArgs;
use log;
use log::{debug, error, trace};
use std::collections::HashMap;
extern crate pretty_env_logger;
use crate::convert_prop_map;
use crate::register_object;
use crate::tks_dbus::collection_impl::CollectionImpl;
use crate::tks_dbus::item_impl::ItemImpl;
use crate::tks_dbus::fdo::collection::register_org_freedesktop_secret_collection;
use crate::tks_dbus::fdo::session::register_org_freedesktop_secret_session;
use crate::tks_dbus::session_impl::SessionImpl;
use crate::tks_dbus::CROSSROADS;

use crate::tks_dbus::DBusHandlePath::SinglePath;
use dbus::arg;
use DBusHandlePath::MultiplePaths;

pub struct ServiceHandle {}
pub struct ServiceImpl {}

impl DBusHandle for ServiceHandle {
    fn path(&self) -> DBusHandlePath {
        SinglePath("/org/freedesktop/secrets".to_string().into())
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
                error!("Error creating session: {:?}", e);
                Err(dbus::MethodErr::failed(&format!(
                    "Error creating session: {:?}",
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
                    register_object!(register_org_freedesktop_secret_session::<SessionImpl>, dh);
                    path
                };
                Ok((output, path.into()))
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
                // no CollectionCreated signal is emitted for the default collection as it is already there
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
            .and_then(|uuid| {
                let coll = CollectionImpl::from(&uuid);
                let collection_path = coll.path();
                register_object!(
                    register_org_freedesktop_secret_collection::<CollectionImpl>,
                    coll
                );
                let collection_path_clone = collection_path.clone();
                tokio::spawn(async move {
                    debug!("Sending CollectionCreated signal");
                    MESSAGE_SENDER.lock().unwrap().send_message(
                        OrgFreedesktopSecretServiceCollectionCreated {
                            collection: collection_path_clone.clone().into(),
                        }
                        .to_emit_message(&collection_path_clone.into()),
                    );
                });
                let prompt_path = dbus::Path::from("/");
                Ok((collection_path.into(), prompt_path))
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
                    .for_each(|c| {
                        $vec.extend(c.items.iter().filter(|i| i.attributes == attributes).map(
                            |i| {
                                ItemImpl::from(i).into()
                            },
                        ));
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
        // TODO: logic with aliases is not correct here; we should instead get paths one by one and
        // compose the unlocked and locked vectors from each step
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
            .filter(|c| {
                collection_names.contains(&c.name)
                    || (c.aliases.as_ref().map_or(false, |aliases| {
                        aliases
                            .iter()
                            .find_map(|n| Some(collection_names.contains(n)))
                            .unwrap()
                    }))
            })
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
                let _ = c.lock();
                match CollectionImpl::from(&*c).path() {
                    SinglePath(p) => locked.push(p),
                    MultiplePaths(mut paths) => locked.append(&mut paths),
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
        SESSION_MANAGER
            .lock()
            .unwrap()
            .sessions
            .get(session_id)
            .ok_or_else(|| {
                error!("Session {} not found", session_id);
                dbus::MethodErr::failed(&"Session not found")
            })?;
        type Secret = (dbus::Path<'static>, Vec<u8>, Vec<u8>, String);
        let secrets_map: HashMap<dbus::Path, Secret> = HashMap::new();
        // TODO as so: iterate over the paths, from each path use DBusHandle::from to grab the
        // corresponding handle, then from that handle go read the secret
        Ok(secrets_map)
    }

    fn read_alias(&mut self, name: String) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        trace!("read_alias {}", name);
        Ok(STORAGE.lock().unwrap().read_alias(&name).map_or_else(
            |_| dbus::Path::from("/"),
            |name| {
                dbus::Path::from(format!(
                    "/org/freedesktop/secrets/collection/{}",
                    sanitize_string(&name)
                ))
            },
        ))
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
        let cols = CollectionImpl::collections()
            .map_err(|e| {
                error!("Error getting collections: {}", e);
                dbus::MethodErr::failed(&format!("Error getting collections"))
            })?
            .iter()
            .map(|c| c.path().into())
            .collect::<Vec<dbus::Path<'static>>>();
        Ok(cols)
    }
}

impl ServiceImpl {
    pub fn new() -> ServiceImpl {
        ServiceImpl {}
    }
    pub fn get_dbus_handle(&self) -> ServiceHandle {
        ServiceHandle {}
    }
    pub fn register_collections() -> Result<(), std::io::Error> {
        let collections = &STORAGE
            .lock()
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting settings: {}", e),
                )
            })?
            .collections;
        collections.iter().for_each(|c| {
            // constructing the CollectionHandle will register the collection
            let _ = CollectionImpl::from(c);
        });
        Ok(())
    }
}
