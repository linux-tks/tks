mod fdo;

// Running these tests require the presence of an active DBus session bus.
// Also, no other service on the DBus should offer org.freeedesktop.secrets.
// Using a DBus session mock obbject would enable running these tests without tinkering with the
// SUT's DBus configuration.
//
#[cfg(test)]
mod tests {
    use crate::fdo::service_client::OrgFreedesktopSecretService;
    use dbus::arg;
    use dbus::arg::Variant;
    use dbus::nonblock;
    use dbus_tokio::connection;
    use regex::Regex;
    use std::env;
    use tks_service::tks_dbus::start_server;
    use tokio::time::sleep;
    use tokio::time::Duration;
    extern crate log;
    extern crate pretty_env_logger;
    use lazy_static::lazy_static;
    use log::{debug, error, info, trace};
    use std::sync::Arc;
    use std::sync::Mutex;

    type ServiceProxy = nonblock::Proxy<'static, Arc<nonblock::SyncConnection>>;
    struct TestFixtureData {
        service_proxy: ServiceProxy,
        stable: bool,
    }

    impl TestFixtureData {
        fn new() -> Self {
            env::set_var("TKS_RUN_MODE", "test");
            env::set_var("RUST_LOG", "debug");
            pretty_env_logger::init();

            let sf = start_server();
            let _ = tokio::spawn(sf);

            let (resource, conn) = connection::new_session_sync().unwrap();
            let _handle = tokio::spawn(async {
                let err = resource.await;
                panic!("Lost connection to D-Bus: {}", err);
            });

            let service_proxy: ServiceProxy = nonblock::Proxy::new(
                "org.freedesktop.secrets",
                "/org/freedesktop/secrets",
                Duration::from_secs(5),
                conn,
            );
            TestFixtureData {
                service_proxy,
                stable: false,
            }
        }
        async fn stable(&mut self) -> &Self {
            match self.stable {
                true => {}
                false => {
                    sleep(Duration::from_millis(300)).await;
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
    #[should_panic]
    async fn test_create_collection_error_no_label() {
        let f = service_proxy!().create_collection(arg::PropMap::new(), "collection1");
        f.await.unwrap();
    }

    #[tokio::test]
    async fn test_create_collection_no_prompt() {
        let mut props = arg::PropMap::new();
        let coll_name = "collection1";
        props.insert(
            "org.freedesktop.Secret.Collection.Label".to_string(),
            Variant(Box::new("collection1".to_string())),
        );
        let f = service_proxy!().create_collection(props, coll_name);
        let (coll_path, prompt_path) = f.await.unwrap();
        debug!("coll_path: {}", coll_path);
        debug!("prompt_path: {}", prompt_path);
        assert!(prompt_path.to_string() == "/");
        assert!(!coll_path.to_string().is_empty());
        let re = Regex::new(r"/org/freedesktop/secrets/collection/(.*)").unwrap();
        assert!(coll_name == &re.captures(&coll_path.to_string()).unwrap()[1]);

        // TODO check that the collection was created on disk
        // TODO checkt that the collections is being reported back by the service
    }
}
