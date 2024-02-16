mod fdo;

// Running these tests require the presence of an active DBus session bus.
// Also, no other service on the DBus should offer org.freeedesktop.secrets.
// Using a DBus session mock obbject would enable running these tests without tinkering with the
// SUT's DBus configuration.
//
#[cfg(test)]
mod tests {
    use crate::fdo::service_client::OrgFreedesktopSecretService;
    use crate::fdo::service_client::OrgFreedesktopSecretServiceCollectionCreated;
    use dbus::arg;
    use dbus::arg::Variant;
    use dbus::nonblock;
    use dbus_tokio::connection;
    use regex::Regex;
    use std::env;
    use tks_service::tks_dbus::start_server;
    use tokio::time::Duration;
    use tokio::time::{interval, sleep};
    extern crate log;
    extern crate pretty_env_logger;
    use futures::executor::block_on;
    use lazy_static::lazy_static;
    use log::{debug, error, info, trace};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread;
    use tks_service::settings::SETTINGS;

    type ServiceProxy = nonblock::Proxy<'static, Arc<nonblock::SyncConnection>>;
    struct TestFixtureData {
        conn: Arc<nonblock::SyncConnection>,
        service_proxy: ServiceProxy,
        stable: bool,
    }

    impl TestFixtureData {
        fn new() -> Self {
            env::set_var("TKS_RUN_MODE", "test");
            env::set_var("RUST_LOG", "debug");
            pretty_env_logger::init();

            let (resource, conn) = connection::new_session_sync().unwrap();
            let _handle = tokio::spawn(async {
                let err = resource.await;
                panic!("Lost connection to D-Bus: {}", err);
            });

            let service_proxy: ServiceProxy = nonblock::Proxy::new(
                "org.freedesktop.secrets",
                "/org/freedesktop/secrets",
                Duration::from_secs(5),
                conn.clone(),
            );
            TestFixtureData {
                conn,
                service_proxy,
                stable: false,
            }
        }
        async fn stable(&mut self) -> &Self {
            match self.stable {
                true => {}
                false => {
                    start_server().await;
                    self.stable = true;
                }
            }
            self
        }
    }

    lazy_static! {
        static ref TEST_FIXTURE_DATA: Arc<Mutex<TestFixtureData>> =
            Arc::new(Mutex::new(TestFixtureData::new()));
    }

    macro_rules! service_proxy {
        () => {
            TEST_FIXTURE_DATA
                .lock()
                .unwrap()
                .stable()
                .await
                .service_proxy
        };
    }

    #[tokio::test]
    async fn test_open_session() {
        let f = service_proxy!().open_session("plain", Variant(Box::new(String::new())));
        let (_, path) = f.await.unwrap();
        let path = path.to_string();
        let re = Regex::new(r"/org/freedesktop/secrets/session/\d+").unwrap();
        assert!(re.is_match(&path));
    }

    #[tokio::test]
    async fn test_read_alias() {
        let f = service_proxy!().read_alias("default");
        let path = f.await.unwrap().to_string();
        assert!(path != "/");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_create_collection_error_no_label() {
        let f = service_proxy!().create_collection(arg::PropMap::new(), "collection1");
        f.await.unwrap();
    }

    #[tokio::test]
    async fn test_create_collection_no_prompt() {
        let coll_name = "collection1";
        let mr_created = dbus::message::MatchRule::new_signal(
            "org.freedesktop.Secret.Service",
            "CollectionCreated",
        );

        let s = service_proxy!().clone();

        {
            let c_clone = s.connection.clone();
            tokio::spawn(async move {
                c_clone.add_match(mr_created).await.unwrap().cb(
                    |_, s: OrgFreedesktopSecretServiceCollectionCreated| {
                        debug!("CollectionCreated: {:?}", s);
                        true
                    },
                );
                debug!("Received CollectionCreated signal");
            });
        }

        let mut props = arg::PropMap::new();
        props.insert(
            "org.freedesktop.Secret.Collection.Label".to_string(),
            Variant(Box::new("collection1".to_string())),
        );
        let f = s.create_collection(props, "");
        let (coll_path, prompt_path) = f.await.unwrap();
        debug!("coll_path: {}", coll_path);
        debug!("prompt_path: {}", prompt_path);
        assert!(prompt_path.to_string() == "/");
        assert!(!coll_path.to_string().is_empty());

        // wait a bit for the signal to arrive
        sleep(Duration::from_millis(300)).await;

        let storage_path: &str = &SETTINGS.lock().unwrap().storage.path;
        assert!(std::path::Path::new(&storage_path).exists());
        let collection_path = format!("{}/{}", storage_path, coll_name);
        assert!(std::path::Path::new(&collection_path).exists());
    }
    // TODO test_create_collection_with_prompt - this should be a case where the collection already
    // exists
}
