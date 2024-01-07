use crate::tks_dbus::fdo::session::OrgFreedesktopSecretSession;
use openssl::pkey::PKey;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use openssl::derive::Deriver;
use openssl::dh::Dh;
use openssl::pkey::Id;
use std::error;
use std::sync::Arc;
use std::sync::Mutex;
use openssl::hash::MessageDigest;
use openssl::md::Md;
use openssl::pkcs5::pbkdf2_hmac;
use openssl::pkey_ctx::PkeyCtx;
use tss_esapi::structures::Digest;
use vec_map::VecMap;

pub struct Session {
    pub id: usize,
    algorithm: String,
    aes_key_bytes: Option<Vec<u8>>,
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
        trace!("Creating new session with algorithm {}", algorithm);
        let mut output = None;
        let sess_id = {
            let session_num = self.next_session_id;
            self.next_session_id += 1;
            let mut session = Session::new(session_num, algorithm.clone());
            output = session.get_shared_secret(input)?;
            self.sessions.insert(session_num, session);
            debug!("Created session {}", session_num);
            session_num
        };
        Ok((sess_id, output))
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
    pub fn new(id: usize, algorithm: String) -> Session {
        Session { id, algorithm, aes_key_bytes: None }
    }
    pub fn get_shared_secret(
        &mut self,
        input: Option<&Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, Box<dyn error::Error>> {
        match self.algorithm.as_str() {
            "plain" => {
                match input {
                    Some(_) => {
                        error!("Algorithm {} does not take input", self.algorithm);
                        return Err("Algorithm does not take input".into());
                    }
                    None => (),
                }
                Ok(None)
            }

            "dh-ietf1024-sha256-aes128-cbc-pkcs7" => {
                match input {
                    Some(input) => {
                        let private_key = PKey::generate_x25519()?;
                        let output = private_key.raw_public_key()?;

                        let peer_key = PKey::public_key_from_raw_bytes(&input[0..output.len()], Id::X25519)?;
                        let mut deriver_1 = Deriver::new(&private_key)?;
                        deriver_1.set_peer(&peer_key)?;
                        let derived_vec = deriver_1.derive_to_vec()?;

                        let mut d2_ctx = PkeyCtx::new_id(Id::HKDF)?;
                        d2_ctx.derive_init()?;
                        d2_ctx.set_hkdf_salt(&[])?;
                        d2_ctx.set_hkdf_md(Md::sha256())?;
                        d2_ctx.add_hkdf_info(&[])?;
                        d2_ctx.set_hkdf_key(derived_vec.as_slice())?;
                        let mut aes_key_bytes: [u8; 16] = [0; 16];
                        let _bytes = d2_ctx.derive(Some(&mut aes_key_bytes))?;
                        self.aes_key_bytes = Some(aes_key_bytes.into());

                        Ok(Some(output))
                    }
                    None => return Err("No input provided".into()),
                }
            }
            _ => {
                error!("Unsupported algorithm: '{}'", self.algorithm);
                return Err("Unsupported algorithm".into());
            }
        }
    }
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
