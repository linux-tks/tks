use crate::register_object;
use crate::tks_dbus::fdo::session::{
    register_org_freedesktop_secret_session, OrgFreedesktopSecretSession,
};
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use std::error;
use std::sync::Arc;
use std::sync::Mutex;
use vec_map::VecMap;

pub fn create_session(
    algorithm: String,
    input: Option<&Vec<u8>>,
) -> Result<(String, Option<Vec<u8>>), Box<dyn error::Error>> {
    trace!("Creating new session with algorithm {}", algorithm);
    SESSION_MANAGER
        .lock()
        .unwrap()
        .new_session(algorithm, input)
}

struct Session {
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
    sessions: Arc<Mutex<VecMap<Session>>>,
}

lazy_static! {
    pub static ref SESSION_MANAGER: Mutex<SessionManager> = Mutex::new(SessionManager::new());
}

impl SessionManager {
    pub fn new() -> SessionManager {
        SessionManager {
            next_session_id: 0,
            sessions: Arc::new(Mutex::new(VecMap::new())),
        }
    }

    fn new_session<'a>(
        &'a mut self,
        algorithm: String,
        input: Option<&Vec<u8>>,
    ) -> Result<(String, Option<Vec<u8>>), Box<dyn error::Error>> {
        debug!("Creating new session with algorithm {}", algorithm);
        match algorithm.as_str() {
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
                let mut ss = self.sessions.lock().unwrap();
                ss.insert(session_num, session);
                trace!("Created session {}", session_num);
                let session = ss.get(session_num).unwrap();
                let sf = session.get_dbus_handle();
                let path = sf.path();
                register_object!(register_org_freedesktop_secret_session::<SessionHandle>, sf);
                Ok((path.to_string(), None))
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
                Err("Not implemented algorithm".into())
            }
            _ => {
                error!("Unsupported algorithm: {}", algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
    fn close_session(&mut self, id: usize) {
        trace!("Closing session {}", id);
        self.sessions.lock().unwrap().remove(id);
    }
}
