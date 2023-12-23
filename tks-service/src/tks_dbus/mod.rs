pub mod fdo;

pub mod collection_impl;
pub mod service_impl;
pub mod session_impl;

use crate::tks_dbus::fdo::service::register_org_freedesktop_secret_service;
use crate::tks_dbus::service_impl::ServiceImpl;
use dbus::channel::MatchingReceiver;
use dbus::channel::Sender;
use dbus::message::MatchRule;
use dbus::*;
use dbus_tokio::connection;
use futures::future;
use lazy_static::lazy_static;
use log::debug;
use std::sync::Arc;
use std::sync::Mutex;

lazy_static! {
    pub static ref CROSSROADS: Arc<Mutex<dbus_crossroads::Crossroads>> =
        Arc::new(Mutex::new(dbus_crossroads::Crossroads::new()));
    pub static ref MESSAGE_SENDER: Arc<Mutex<MessageSender>> =
        Arc::new(Mutex::new(MessageSender::new()));
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
            let p = $f.path().to_string();
            debug!("Registering {}", p);
            {
                let mut cr_lock = CROSSROADS.lock().unwrap();
                let itf = $iface(&mut cr_lock);
                cr_lock.insert($f.path(), &[itf], $f);
            }
            debug!("Registered {}", p);
        });
    };
}

pub async fn start_server() {
    debug!("Connecting to the D-Bus session bus");
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
        debug!("Registering org.freedesktop.Secret.Service");
        let mut crossroads = CROSSROADS.lock().unwrap();
        let itf = register_org_freedesktop_secret_service(&mut crossroads);
        crossroads.insert("/org/freedesktop/secrets", &[itf], ServiceImpl::new());
    }

    let nr = c
        .request_name("org.freedesktop.secrets", false, true, true)
        .await
        .unwrap_or_else(|_| {
            panic!("Failed to acquire the service name");
        });
    use dbus::nonblock::stdintf::org_freedesktop_dbus::RequestNameReply::*;
    debug!("Request name reply: {:?}", nr);
    if nr != PrimaryOwner {
        panic!("Failed to acquire the service name");
    }

    debug!("Start serving");
    c.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            debug!("Received message: {:?}", msg);
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
    future::pending::<()>().await;
    unreachable!();
}
