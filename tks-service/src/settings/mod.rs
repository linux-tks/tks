use config::{Config, ConfigError, Environment, File};
use lazy_static::lazy_static;
use log::debug;
use serde_derive::Deserialize;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Storage {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Settings {
    pub storage: Storage,
}

lazy_static! {
    pub static ref SETTINGS: Arc<Mutex<Settings>> = Arc::new(Mutex::new(
        Settings::new().expect("Failed to read settings.")
    ));
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("TKS_RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            .add_source(File::with_name("local").required(false))
            .add_source(Environment::with_prefix("tks"))
            // You may also programmatically change settings
            // .set_override("database.url", "postgres://")?
            .build()?;

        debug!("configuration: {:?}", s);

        match s.try_deserialize() {
            Ok(s) => {
                let mut settings: Settings = s;
                settings.storage.path = shellexpand::full(&settings.storage.path)
                    .expect("Failed to expand storage path.")
                    .into_owned()
                    .into();
                Ok(settings)
            }
            Err(e) => {
                debug!("Failed to deserialize settings: {:?}", e);
                Err(e)
            }
        }
    }
}
