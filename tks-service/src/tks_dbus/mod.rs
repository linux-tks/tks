pub mod fdo;

pub mod service_impl;
pub mod session_impl;

use crate::tks_dbus::fdo::service::register_org_freedesktop_secret_service;
use crate::tks_dbus::service_impl::ServiceImpl;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus_tokio::connection;
use futures::future;
use lazy_static::lazy_static;
use log::debug;
use std::sync::Arc;
use std::sync::Mutex;

lazy_static! {
    pub static ref CROSSROADS: Arc<Mutex<dbus_crossroads::Crossroads>> =
        Arc::new(Mutex::new(dbus_crossroads::Crossroads::new()));
}

#[macro_export]
macro_rules! register_object {
    ($iface:expr, $f:expr) => {
        tokio::spawn(async {
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

    {
        debug!("Registering org.freedesktop.Secret.Service");
        let mut crossroads = CROSSROADS.lock().unwrap();
        // crossroads.set_async_support(Some((
        //     c.clone(),
        //     Box::new(|x| {
        //         tokio::spawn(x);
        //     }),
        // )));
        let itf = register_org_freedesktop_secret_service(&mut crossroads);
        crossroads.insert("/org/freedesktop/secrets", &[itf], ServiceImpl {});
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

// pub fn add(left: usize, right: usize) -> usize {
//     left + right
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
