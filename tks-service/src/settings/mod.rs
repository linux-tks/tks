use crate::tks_error::TksError;
use config::{Config, Environment, File};
use lazy_static::lazy_static;
use log::debug;
use serde_derive::Deserialize;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Storage {
    pub path: Option<String>,
    /// see [StorageBackendType]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
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
    pub const XDG_DIR_NAME: &'static str = "io.linux-tks";
    pub fn new() -> Result<Self, TksError> {
        // let run_mode = env::var("TKS_RUN_MODE").unwrap_or_else(|_| "development".into());

        let xdg_dirs = xdg::BaseDirectories::with_prefix(Settings::XDG_DIR_NAME)?;
        let config_path = xdg_dirs
            .place_config_file("service.toml")
            .expect("Failed to place config file.");
        let s = Config::builder()
            .add_source(File::with_name(
                config_path
                    .to_str()
                    .ok_or_else(|| TksError::ConfigurationError("".to_string()))?,
            ))
            .add_source(File::with_name("local").required(false))
            .add_source(Environment::with_prefix("tks"))
            .set_default("storage.backend", "fscrypt")?
            // .set_default("storage.path",
            //              xdg_dirs.create_data_directory("storage")?
            //                  .to_str())?
            .build()?;

        debug!("configuration: {:?}", s);

        s.try_deserialize()
            .and_then(|s| {
                let mut settings: Settings = s;
                if !settings.storage.path.is_none() {
                    settings.storage.path = Some(shellexpand::full(&settings
                        .storage.path.unwrap())
                        .expect("Failed to expand storage path.")
                        .into_owned()
                        .into());
                }
                Ok(settings)
            })
            .map_err(|e| TksError::ConfigurationError(e.to_string()))
    }
}
