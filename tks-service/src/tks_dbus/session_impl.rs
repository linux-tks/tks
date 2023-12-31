use crate::tks_dbus::fdo::session::OrgFreedesktopSecretSession;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use std::error;
use std::sync::Arc;
use std::sync::Mutex;
use vec_map::VecMap;

pub struct Session {
    pub id: usize,
    algorithm: String,
}

#[derive(Debug)]
pub struct SessionHandle {
    id: usize,
    pub encrypted_output: Option<EncryptedOutput>,
}

#[derive(Debug)]
pub struct EncryptedOutput {
    pub data: String,
}

impl OrgFreedesktopSecretSession for SessionHandle {
    fn close(&mut self) -> Result<(), dbus::MethodErr> {
        SESSION_MANAGER.lock().unwrap().close_session(self.id);
        CROSSROADS
            .lock()
            .unwrap()
            .remove::<SessionHandle>(&self.path());
        Ok(())
    }
}

impl DBusHandle for SessionHandle {
    fn path(&self) -> dbus::Path<'static> {
        format!("/org/freedesktop/secrets/session/{}", self.id).into()
    }
}

impl Session {
    pub fn get_dbus_handle(&self) -> SessionHandle {
        SessionHandle {
            id: self.id,
            encrypted_output: None,
        }
    }
}

pub struct SessionManager {
    next_session_id: usize,
    pub sessions: VecMap<Session>,
}

lazy_static! {
    pub static ref SESSION_MANAGER: Arc<Mutex<SessionManager>> =
        Arc::new(Mutex::new(SessionManager::new()));
}

impl SessionManager {
    pub fn new() -> SessionManager {
        SessionManager {
            next_session_id: 0,
            sessions: VecMap::new(),
        }
    }

    pub fn new_session(
        &mut self,
        algorithm: String,
        input: Option<&Vec<u8>>,
    ) -> Result<(usize, Option<Vec<u8>>), Box<dyn error::Error>> {
        debug!("Creating new session with algorithm {}", algorithm);
        let sess_id = match algorithm.as_str() {
            "plain" => {
                match input {
                    Some(_) => {
                        error!("Algorithm {} does not take input", algorithm);
                        return Err("Algorithm does not take input".into());
                    }
                    None => (),
                }
                let session_num = self.next_session_id;
                self.next_session_id += 1;
                let session = Session {
                    id: session_num,
                    algorithm: algorithm.clone(),
                };
                self.sessions.insert(session_num, session);
                trace!("Created session {}", session_num);
                session_num
            }

            "dh-ietf1024-sha256-aes128-cbc-pkcs7" => {
                // let input = input.0;
                // let input = input.as_iter().unwrap();
                // let input = input.collect::<Vec<&dyn arg::RefArg>>();
                // let input = input[0].as_str().unwrap();
                // info!("input: {}", input);
                // let output = String::from("output");
                // Ok((sessions.len() - 1, Some(output)))
                error!("Algorithm {} not implemented", algorithm);
                return Err("Not implemented algorithm".into());
            }
            _ => {
                error!("Unsupported algorithm: {}", algorithm);
                return Err("Unsupported algorithm".into());
            }
        };
        Ok((sess_id, None))
    }
    fn close_session(&mut self, id: usize) {
        debug!("Closing session {}", id);
        self.sessions.remove(id);
    }
}

impl DBusHandle for SessionManager {
    fn path(&self) -> dbus::Path<'static> {
        "/org/freedesktop/secrets/session".into()
    }
}

impl Session {
    pub fn decrypt(&self, input: &Vec<u8>) -> Result<Vec<u8>, Box<dyn error::Error>> {
        debug!("Decrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            "plain" => Ok(input.clone()),
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
    pub fn encrypt(&self, input: &Vec<u8>) -> Result<Vec<u8>, Box<dyn error::Error>> {
        debug!("Encrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            "plain" => Ok(input.clone()),
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
}
