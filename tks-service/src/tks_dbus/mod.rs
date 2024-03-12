pub mod fdo;

pub mod collection_impl;
pub mod item_impl;
pub mod prompt_impl;
pub mod service_impl;
pub mod session_impl;

use crate::tks_dbus::fdo::service::register_org_freedesktop_secret_service;
use crate::tks_dbus::service_impl::ServiceImpl;
use dbus::channel::MatchingReceiver;
use dbus::channel::Sender;
use dbus::message::MatchRule;
use dbus::*;
use dbus_tokio::connection;
use lazy_static::lazy_static;
use log::{debug, trace, warn};
use std::sync::Arc;
use std::sync::Mutex;

lazy_static! {
    pub static ref CROSSROADS: Arc<Mutex<dbus_crossroads::Crossroads>> =
        Arc::new(Mutex::new(dbus_crossroads::Crossroads::new()));
    pub static ref MESSAGE_SENDER: Arc<Mutex<MessageSender>> =
        Arc::new(Mutex::new(MessageSender::new()));
}

#[derive(Clone)]
pub enum DBusHandlePath {
    SinglePath(dbus::Path<'static>),
    MultiplePaths(Vec<dbus::Path<'static>>),
}

impl From<DBusHandlePath> for dbus::Path<'static> {
    fn from(p: DBusHandlePath) -> Self {
        match p {
            DBusHandlePath::SinglePath(p) => p,
            DBusHandlePath::MultiplePaths(v) => {
                warn!(
                    "This is a DBusPath having multiple paths, returning the first one: {}",
                    v[0]
                );
                v[0].clone()
            }
        }
    }
}

pub trait DBusHandle {
    fn path(&self) -> DBusHandlePath;
}

// https://dbus.freedesktop.org/doc/dbus-specification.html#message-protocol-marshaling-object-path
fn sanitize_string(s: &str) -> String {
    assert!(!s.is_empty());
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' => c,
            _ => '_',
        })
        .collect()
}

pub struct MessageSender {
    connection: Option<Arc<nonblock::SyncConnection>>,
}

impl MessageSender {
    fn new() -> Self {
        MessageSender { connection: None }
    }
    fn set_connection(&mut self, connection: Arc<nonblock::SyncConnection>) {
        self.connection = Some(connection);
    }
    pub fn send_message(&self, msg: Message) {
        debug!("Sending message: {:?}", msg);
        match &self.connection {
            Some(c) => {
                c.send(msg).unwrap();
            }
            None => {
                panic!("No connection");
            }
        }
    }
}

#[macro_export]
macro_rules! register_object {
    ($iface:expr, $f:expr) => {
        tokio::spawn(async move {
            {
                let mut cr_lock = CROSSROADS.lock().unwrap();
                let itf = $iface(&mut cr_lock);
                match $f.path() {
                    DBusHandlePath::SinglePath(p) => {
                        let p = p.to_string();
                        trace!("Registering {}", p);
                        cr_lock.insert(p, &[itf], $f);
                    }
                    DBusHandlePath::MultiplePaths(paths) => {
                        for p in paths {
                            let ps = p.to_string();
                            trace!("Registering {}", ps);
                            cr_lock.insert(p, &[itf], $f.clone());
                        }
                    }
                }
            }
        });
    };
}

#[macro_export]
macro_rules! convert_prop_map {
    ($properties:expr) => {
        // the DBus spec says that properties is a dict of string:variant, but really it should be
        // a dict of string:String
        {
            let mut errors = Vec::new();
            let string_props: HashMap<String, String> = $properties
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
            (string_props, errors)
        }
    };
}

const DBUS_NAME: &'static str = "org.freedesktop.secrets";

const DBUS_PATH: &'static str = "/org/freedesktop/secrets";

pub async fn start_server() {
    trace!("Connecting to the D-Bus session bus");
    let (resource, c) = connection::new_session_sync().unwrap_or_else(|_| {
        panic!(
            "Failed to connect to the D-Bus session bus. \
                 Is a session bus instance of D-Bus running?"
        )
    });
    let _handle = tokio::spawn(async {
        let err = resource.await;
        panic!("Connection has died: {:?}", err);
    });

    MESSAGE_SENDER.lock().unwrap().set_connection(c.clone());

    {
        trace!("Registering org.freedesktop.Secret.Service");
        let mut crossroads = CROSSROADS.lock().unwrap();
        let itf = register_org_freedesktop_secret_service(&mut crossroads);
        let service = ServiceImpl::new();
        crossroads.insert(DBUS_PATH, &[itf], service);
        ServiceImpl::register_collections().unwrap();
    }

    trace!("Requesting name {}", DBUS_NAME);
    let nr = c
        .request_name(DBUS_NAME, false, true, true)
        .await
        .unwrap_or_else(|_| {
            panic!("Failed to acquire the service name");
        });
    use dbus::nonblock::stdintf::org_freedesktop_dbus::RequestNameReply::*;
    debug!("Request name reply: {:?}", nr);
    if nr != PrimaryOwner {
        panic!("Failed to acquire the service name");
    }

    // let proxy = Proxy::new("org.freedesktop.DBus.Local", "/org/freedesktop/DBus/Local", Default::default(), c);
    // tokio::spawn( async move {
    //     proxy.
    // });

    trace!("Start serving");
    c.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            trace!("Received message: {:?}", msg);
            {
                CROSSROADS
                    .lock()
                    .unwrap()
                    .handle_message(msg, conn)
                    .unwrap();
            }
            debug!("Handled message");
            true
        }),
    );
    trace!("Start receiving signals");
    c.start_receive(
        MatchRule::new_signal("org.freedesktop.DBus.local", "Disconnected"),
        Box::new(move |msg, _conn| {
            trace!("Received signal: {:?}", msg);
            true
        }),
    );
}
