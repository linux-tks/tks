use crate::storage::STORAGE;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretService;
use crate::tks_dbus::fdo::service::OrgFreedesktopSecretServiceCollectionCreated;
use crate::tks_dbus::session_impl::SESSION_MANAGER;
use crate::tks_dbus::{sanitize_string, DBusHandle};
use crate::tks_dbus::{DBusHandlePath, MESSAGE_SENDER};
use dbus::message::SignalArgs;
use log;
use log::{debug, error, trace};
use std::collections::{HashMap, VecDeque};
use std::ops::Deref;

extern crate pretty_env_logger;
use crate::convert_prop_map;
use crate::register_object;
use crate::tks_dbus::collection_impl::CollectionImpl;
use crate::tks_dbus::fdo::collection::{
    register_org_freedesktop_secret_collection, OrgFreedesktopSecretCollection,
};
use crate::tks_dbus::fdo::session::register_org_freedesktop_secret_session;
use crate::tks_dbus::item_impl::ItemImpl;
use crate::tks_dbus::session_impl::SessionImpl;
use crate::tks_dbus::CROSSROADS;

use crate::tks_dbus::client_context::{TksClientOption, TksClientProcess, CLIENT_REGISTRY};
use crate::tks_dbus::fdo::item::OrgFreedesktopSecretItem;
use crate::tks_dbus::prompt_impl::{PromptWithPinentry, TksPromptChain};
use crate::tks_dbus::DBusHandlePath::SinglePath;
use crate::tks_error::TksError;
use dbus::arg;
use dbus_crossroads::{Context, PropContext};
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
        ctx: &mut Context,
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
        Ok(sm
            .new_session(algorithm, arg::cast(&input.0), ctx.message().sender())
            .and_then(|(sess_id, vector)| {
                let output = vector.map_or_else(
                    || arg::Variant(Box::new(String::new()) as Box<dyn arg::RefArg + 'static>),
                    |v| arg::Variant(Box::new(v.clone()) as Box<dyn arg::RefArg + 'static>),
                );
                let path = {
                    let dh = sm.sessions.get(sess_id).unwrap().get_dbus_handle();
                    let path = dh.path();
                    register_object!(register_org_freedesktop_secret_session::<SessionImpl>, dh);
                    path
                };
                Ok((output, path.into()))
            })?)
    }

    /// Create a new collection
    /// # Arguments
    /// * `properties` - A HashMap of properties to set on the collection; this version ignores any
    /// properties but the org.freedesktop.Secret.Collection.Label property, which is required
    /// * `alias` - The alias to use for the collection
    fn create_collection(
        &mut self,
        ctx: &mut Context,
        properties: arg::PropMap,
        alias: String,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("create_collection alias={}", alias);

        match alias.as_str() {
            "default" => {
                // no CollectionCreated signal is emitted for the default collection as it is already there
                // TODO add any new properties to the existing collection
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
                e.into()
            })
    }
    fn search_items(
        &mut self,
        ctx: &mut Context,
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
                        $vec.extend(
                            c.items
                                .iter()
                                .filter(|i| {
                                    attributes.iter().fold(true, |b, (k, v)| {
                                        b && i
                                            .attributes
                                            .clone()
                                            .into_keys()
                                            .find(|kx| kx == k)
                                            .is_some()
                                            && i.attributes
                                                .clone()
                                                .into_values()
                                                .find(|vx| vx == v)
                                                .is_some()
                                    })
                                })
                                .map(|i| ItemImpl::from(i).into()),
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
        ctx: &mut Context,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr> {
        trace!("unlock {:?}, sender: {:?}", objects, ctx.message().sender());
        let mut prompts = VecDeque::new();

        let mut binding = CLIENT_REGISTRY.lock().unwrap();
        let client_opt = binding.retrieve(ctx)?;
        let client;
        match client_opt {
            TksClientOption::Prompt(prompt) => prompts.push_back(dbus::Path::from(prompt)),
            TksClientOption::Client(c) => client = c.clone(),
        }

        let collection_paths: Vec<_> = if objects.is_empty() {
            let default_collection_path =
                dbus::Path::from("/org/freedesktop/secrets/aliases/default");
            let mut collection_paths = Vec::new();
            collection_paths.push((
                default_collection_path.clone(),
                default_collection_path.clone(),
                CollectionImpl::from(&default_collection_path),
            ));
            collection_paths
        } else {
            let collection_paths = objects
                .iter()
                .map(|p| {
                    let cp: Vec<_> = p.split('/').collect();
                    let cp = cp[0..6].join("/");
                    let cp = dbus::Path::from(cp);
                    let coll = CollectionImpl::from(&cp);
                    (p.clone(), cp, coll)
                })
                .collect();
            collection_paths
        };
        let mut unlocked = Vec::new();
        let no_prompt = dbus::Path::from("/");
        for cc in collection_paths {
            let coll = cc.2;
            if coll.locked()? {
                let unlock_action = STORAGE.lock().unwrap().create_unlock_action(&coll.uuid)?;
                let prompt = PromptWithPinentry::new(unlock_action)?;
                prompts.push_back(dbus::Path::from(prompt));
            } else {
                unlocked.push(cc.1);
            }
        }
        debug!("unlocked: {:?}, prompt: {:?}", unlocked, prompts);
        let returned_prompt = match prompts.len() {
            0 => no_prompt,
            1 => prompts.pop_front().unwrap(),
            _ => TksPromptChain::new(prompts),
        };
        let unlocked = unlocked;
        Ok((unlocked, returned_prompt))
    }

    fn lock(
        &mut self,
        ctx: &mut Context,
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
        ctx: &mut Context,
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
        type Secret = (dbus::Path<'static>, Vec<u8>, Vec<u8>, String);
        let mut secrets_map: HashMap<dbus::Path, Secret> = HashMap::new();

        let items: Vec<_> = items.iter().map(|p| ItemImpl::from(p)).collect();
        for mut i in items {
            secrets_map.insert(i.path.clone(), i.get_secret(session.clone(), ctx)?);
        }
        Ok(secrets_map)
    }

    fn read_alias(
        &mut self,
        ctx: &mut Context,
        name: String,
    ) -> Result<dbus::Path<'static>, dbus::MethodErr> {
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
        ctx: &mut Context,
        _name: String,
        _collection: dbus::Path<'static>,
    ) -> Result<(), dbus::MethodErr> {
        trace!("Hello from set_alias");
        return Err(dbus::MethodErr::failed(&format!(
            "Error setting alias: {}",
            "Not implemented"
        )));
    }
    fn collections(
        &self,
        ctx: &mut PropContext,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr> {
        trace!("collections");
        let cols = CollectionImpl::collections()?
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
    pub fn register_collections() -> Result<(), TksError> {
        let collections = &STORAGE.lock()?.collections;
        collections.iter().for_each(|c| {
            // constructing the CollectionHandle will register the collection
            let _ = CollectionImpl::from(c);
        });
        Ok(())
    }
}
