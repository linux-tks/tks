mod fdo;

// Running these tests require the presence of an active DBus session bus.
// Also, no other service on the DBus should offer org.freeedesktop.secrets.
// Using a DBus session mock obbject would enable running these tests without tinkering with the
// SUT's DBus configuration.
//
#[cfg(test)]
mod tests {
    use tks_service::tks_dbus::start_server;
    use tokio::time::{error::Elapsed, timeout, Duration};

    #[tokio::test]
    #[sequential_test::sequential]
    async fn test_start_server() {
        let sf = start_server();
        let sh = tokio::spawn(sf);

        let result = timeout(Duration::from_secs(1), sh).await;

        match result {
            Ok(_) => {
                panic!("Server should never return when started without errors");
            }
            Err(Elapsed { .. }) => {
                println!("Server started without error");
            }
        }
    }

    use dbus::nonblock;

    #[tokio::test]
    #[sequential_test::sequential]
    async fn test_service() {
        use crate::fdo::service_client::OrgFreedesktopSecretService;
        use dbus::arg;
        use dbus::arg::Variant;
        use dbus_tokio::connection;
        use regex::Regex;
        use tokio::time::sleep;

        let sf = start_server();
        let sh = tokio::spawn(sf);
        sleep(Duration::from_secs(1)).await;

        let (resource, conn) = connection::new_session_sync().unwrap();
        let handle = tokio::spawn(async {
            let err = resource.await;
            panic!("Lost connection to D-Bus: {}", err);
        });

        let proxy = nonblock::Proxy::new(
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets",
            Duration::from_secs(5),
            conn,
        );

        let (_, path) = proxy
            .open_session("plain", Variant(Box::new(String::new())))
            .await
            .unwrap();
        let path = path.to_string();
        let re = Regex::new(r"/org/freedesktop/secrets/session/\d+").unwrap();
        assert!(re.is_match(&path));

        // create a collection
        let (collection, _prompt) = proxy
            .create_collection(arg::PropMap::new(), "collection1")
            .await
            .unwrap();
        // test collections method_call
        let collections = proxy.collections().await.unwrap();

        handle.abort();
    }
}
