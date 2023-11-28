pub struct SessionImpl;
use dbus_crossroads as crossroads;
use lazy_static::lazy_static;
use log::{error, info, trace};
use std::error;
use std::sync::Mutex;

use crate::tks_dbus::fdo::session::{
    register_org_freedesktop_secret_session, OrgFreedesktopSecretSession,
};

impl OrgFreedesktopSecretSession for SessionImpl {
    fn close(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("Hello from close");
        Ok(())
    }
}

fn register_session(cr: &mut dbus_crossroads::Crossroads) {
    trace!("Registering org.freedesktop.Secret.Session");
    let tok: crossroads::IfaceToken<SessionImpl> = register_org_freedesktop_secret_session(cr);
    cr.insert("/org/freedesktop/secrets/session/s0", &[tok], SessionImpl);
    trace!("Registered org.freedesktop.Secret.Session");
}

struct Session {
    algorithm: String,
}

struct SessionManager {
    sessions: Vec<Session>,
}

impl SessionManager {
    fn new_session(
        &mut self,
        algorithm: String,
        input: Option<&Vec<u8>>,
    ) -> Result<(usize, Option<Vec<u8>>), Box<dyn error::Error>> {
        trace!("Creating new session with algorithm {}", algorithm);
        match algorithm.as_str() {
            "" => {
                match input {
                    Some(_) => {
                        error!("Algorithm {} does not take input", algorithm);
                        return Err("Algorithm does not take input".into());
                    }
                    None => (),
                }
                &self.sessions.push(Session { algorithm });
                let session_num = &self.sessions.len() - 1;
                trace!("Created session {}", session_num);
                Ok((session_num, None))
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
}

lazy_static! {
    static ref SESSION_MANAGER: Mutex<SessionManager> = Mutex::new(SessionManager {
        sessions: Vec::new()
    });
}

pub fn create_session(
    algorithm: String,
    input: Option<&Vec<u8>>,
) -> Result<(usize, Option<Vec<u8>>), Box<dyn error::Error>> {
    trace!("Creating new session with algorithm {}", algorithm);
    SESSION_MANAGER
        .lock()
        .unwrap()
        .new_session(algorithm, input)
}
