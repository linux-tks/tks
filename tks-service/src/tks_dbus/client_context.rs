use crate::tks_dbus::prompt_impl::{
    ConfirmationMessageActionParam, PromptAction, PromptDialog, PromptWithPinentry, TksPrompt,
};
use crate::tks_error::TksError;
use dbus::arg::{PropMap, RefArg, Variant};
use dbus_crossroads::Context;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use openssl::sha;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::Pid;
use sysinfo::ProcessRefreshKind;
use sysinfo::RefreshKind;
use tokio::task;

#[derive(Clone, Debug)]
pub struct TksClientProcess {
    name: String,
    exe_path: OsString,
}

pub enum TksClientOption {
    Prompt(String),
    Client(TksClient),
}

/// Information about the TKS client process
/// TODO hold the calling process binary SHA (and have it automatically updated upon system update?)
/// TODO retrieve method below should check actuall caller has the same SHA as when enrolled
#[derive(Clone, Debug)]
pub struct TksClient {}

pub struct EnrollClientPrompt {
    client_process: TksClientProcess,
}

impl TksPrompt for EnrollClientPrompt {
    fn prompt(
        &self,
        _window_id: String,
    ) -> Result<(bool, Option<VecDeque<dbus::Path<'static>>>), TksError> {
        todo!()
    }

    fn dismiss(&self) -> Result<(), TksError> {
        todo!()
    }
}

impl EnrollClientPrompt {
    pub fn new(client: &TksClientProcess) -> EnrollClientPrompt {
        EnrollClientPrompt {
            client_process: client.clone(),
        }
    }
}

/// This holds the known clients
/// TODO store contents encrypted on disk and load it upon service start
pub struct ClientRegistry {
    known_clients: HashMap<OsString, TksClient>,
}

impl ClientRegistry {
    fn new() -> ClientRegistry {
        ClientRegistry {
            known_clients: HashMap::new(),
        }
    }
    pub fn retrieve(
        self: &mut ClientRegistry,
        ctx: &mut Context,
    ) -> Result<TksClientOption, TksError> {
        let process = TksClientProcess::new(ctx)?;

        match self.known_clients.get(&process.exe_path) {
            Some(client) => {
                // TODO also check the client process executable's SHA to
                // ensure no spoofing is taking place
                Ok(TksClientOption::Client(client.clone()))
            }
            None => {
                // new client process
                let action = PromptAction {
                    dialog: PromptDialog::ConfirmationMessage(
                        "Yes".into(),
                        "No".into(),
                        format!(
                            "An application having the process \
                        executable {:?} wants to let Tks handle their secrets\
                        . Should we accept this?",
                            process.exe_path
                        )
                        .into(),
                        ConfirmationMessageActionParam::ConfirmNewClient(process.exe_path),
                        |param| {
                            match param {
                                ConfirmationMessageActionParam::ConfirmNewClient(exe_path) => {
                                    trace!("Registering client {}", exe_path.to_string_lossy());
                                    // TODO we should check if meanwhile a same path client has been added here
                                    // and that it is the same SHA; if not, then dismiss the operation
                                    let client = TksClient {};
                                    CLIENT_REGISTRY
                                        .lock()
                                        .unwrap()
                                        .known_clients
                                        .insert(exe_path.clone(), client);
                                    Ok(false) // we succeeded, but we don't dismiss this dialog
                                }
                                _ => {
                                    error!("Unexpected confirmation message param: {:?}", param);
                                    assert!(false);
                                    Ok(true)
                                }
                            }
                        },
                    ),
                };
                Ok(TksClientOption::Prompt(
                    PromptWithPinentry::new(action)?.to_string(),
                ))
            }
        }
    }
}

lazy_static! {
    pub static ref CLIENT_REGISTRY: Arc<Mutex<ClientRegistry>> =
        Arc::new(Mutex::new(ClientRegistry::new()));
}

impl TksClientProcess {
    pub fn new(ctx: &mut Context) -> Result<TksClientProcess, TksError> {
        let name = ctx
            .message()
            .sender()
            .ok_or_else(|| TksError::ContextError("Cannot get message sender"))
            .unwrap()
            .to_string();
        let conn = dbus::blocking::Connection::new_session()?;
        let proxy = conn.with_proxy(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            Duration::from_secs(5),
        );
        let (credentials,): (PropMap,) = proxy.method_call(
            "org.freedesktop.DBus",
            "GetConnectionCredentials",
            (name.clone(),),
        )?;
        debug!("Obtained dbus credentials {:?}", credentials);

        let s = sysinfo::System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        let caller_process = s
            .process(Pid::from_u32(
                credentials
                    .get("ProcessID")
                    .ok_or_else(|| TksError::ContextError("No ProcessID found"))?
                    .as_i64()
                    .ok_or_else(|| TksError::ContextError("No Process ID number"))?
                    as u32,
            ))
            .ok_or_else(|| TksError::ContextError("No Process ID number"))?;
        debug!("Caller process: {:?}", caller_process);
        let exe_path = caller_process
            .exe()
            .ok_or_else(|| TksError::ContextError("No EXE path"))?;
        debug!("Caller process path: {:?}", exe_path);

        let mut hasher = sha::Sha256::new();
        let mut exe_file = std::fs::File::open(exe_path)?;
        loop {
            let mut chunk = vec![0u8; 1024];
            let n = exe_file.read(&mut chunk)?;
            if n == 0 {
                break;
            };
            hasher.update(chunk.as_slice());
        }
        let exe_sha = hasher.finish();
        debug!("Call process hash: {:?}", exe_sha);

        Ok(TksClientProcess {
            name,
            exe_path: exe_path.into(),
        })
    }
}
