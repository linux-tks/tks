mod fdo;
mod service_impl;
mod session_impl;

use crate::tks_dbus::fdo::service::register_org_freedesktop_secret_service;
use crate::tks_dbus::service_impl::ServiceImpl;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus_tokio::connection;
use futures::future;
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
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

// #[cfg(test)]
// mod tests {
//     TODO - figure out how to test this as for the moment it indefinitely hangs
//     use super::*;
//     use std::sync::Arc;
//     use std::sync::Mutex;
//     use std::thread;
//     use tokio::time::{error::Elapsed, timeout, Duration};
//     #[tokio::test]
//     async fn test_start_server() {
//         let start_status = Arc::new(Mutex::new(false));
//
//         let result = timeout(Duration::from_secs(10), async {
//             let start_status = Arc::clone(&start_status);
//             let c = thread::spawn(move || {
//                 let mut result = start_status.lock().unwrap();
//                 *result = match start_server() {
//                     Ok(_) => true,
//                     Err(e) => {
//                         error!("Server failed to start: {}", e);
//                         false
//                     }
//                 }
//             });
//             c.join().unwrap();
//         })
//         .await;
//
//         match result {
//             Ok(_) => {
//                 panic!("Server should never return when started without errors");
//             }
//             Err(Elapsed { .. }) => {
//                 assert!(start_status.lock().unwrap().clone());
//             }
//             Err(e) => {
//                 panic!("Server failed to start: {}", e);
//             }
//         }
//     }
//     #[tokio::main]
//     async fn main() {}
// }
