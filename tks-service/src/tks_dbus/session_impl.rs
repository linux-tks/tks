use crate::tks_dbus::fdo::session::OrgFreedesktopSecretSession;
use crate::tks_dbus::DBusHandlePath::SinglePath;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::{DBusHandle, DBusHandlePath};
use lazy_static::lazy_static;
use log::{debug, error, trace};
use openssl::bn::BigNum;
use openssl::dh::Dh;
use openssl::md::Md;
use openssl::pkey::Id;
use openssl::pkey_ctx::{HkdfMode, PkeyCtx};
use openssl::symm::{Cipher, decrypt, encrypt};
use std::error;
use std::sync::Arc;
use std::sync::Mutex;
use vec_map::VecMap;
use crate::TksError;

pub struct Session {
    pub id: usize,
    algorithm: String,
    aes_key_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct SessionImpl {
    id: usize,
}

#[derive(Debug)]
pub struct EncryptedOutput {
    pub data: String,
}

impl OrgFreedesktopSecretSession for SessionImpl {
    fn close(&mut self) -> Result<(), dbus::MethodErr> {
        SESSION_MANAGER.lock().unwrap().close_session(self.id);
        CROSSROADS
            .lock()
            .unwrap()
            .remove::<SessionImpl>(&self.path().into());
        Ok(())
    }
}

impl DBusHandle for SessionImpl {
    fn path(&self) -> DBusHandlePath {
        SinglePath(format!("/org/freedesktop/secrets/session/{}", self.id).into())
    }
}

impl Session {
    pub fn get_dbus_handle(&self) -> SessionImpl {
        SessionImpl { id: self.id }
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
    ) -> Result<(usize, Option<Vec<u8>>), TksError> {
        trace!("Creating new session with algorithm {}", algorithm);
        let output;
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
    fn path(&self) -> DBusHandlePath {
        SinglePath("/org/freedesktop/secrets/session".into())
    }
}

const DH_AES: &'static str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";
// const X25519: &'static str = "x25519";
const PLAIN: &'static str = "plain";

impl Session {
    pub fn new(id: usize, algorithm: String) -> Session {
        Session {
            id,
            algorithm,
            aes_key_bytes: None,
        }
    }
    pub fn get_shared_secret(
        &mut self,
        input: Option<&Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, TksError> {
        match self.algorithm.as_str() {
            PLAIN => {
                if let Some(_) = input {
                    error!("Algorithm {} does not take input", self.algorithm);
                    Err(TksError::ParameterError)
                } else {
                    Ok(None)
                }
            }

            DH_AES => {
                if let Some(input) = input {
                    let p = BigNum::get_rfc2409_prime_1024()?;
                    let g = BigNum::from_u32(2u32)?;
                    let dh = Dh::from_pqg(p, None, g)?;
                    let priv_key = dh.generate_key()?;
                    let pub_key = priv_key.public_key();

                    let client_pub_key = BigNum::from_slice(input.as_slice())?;
                    let shared_secret = priv_key.compute_key(&client_pub_key)?;

                    let mut derive_key = PkeyCtx::new_id(Id::HKDF)?;
                    derive_key.derive_init()?;
                    derive_key.set_hkdf_mode(HkdfMode::EXTRACT_THEN_EXPAND)?;
                    let salt: [u8; 32] = [0; 32];
                    derive_key.set_hkdf_salt(&salt)?;
                    derive_key.set_hkdf_md(Md::sha256())?;
                    derive_key.set_hkdf_key(shared_secret.as_slice())?;
                    let mut aes_bytes = vec![0u8; 128];
                    derive_key.derive(Some(aes_bytes.as_mut_slice()))?;
                    self.aes_key_bytes = Some(aes_bytes[..16].to_owned());

                    Ok(Some(pub_key.to_vec()))
                } else {
                    Err(TksError::ParameterError)
                }
            }
            // X25519 => {
            //     if let Some(input) = input {
            //         let peer_key = PKey::public_key_from_raw_bytes(&input, Id::X25519)?;
            //
            //         let private_key = PKey::generate_x25519()?;
            //         let mut deriver_1 = Deriver::new(&private_key)?;
            //         deriver_1.set_peer(&peer_key)?;
            //         let derived_vec = deriver_1.derive_to_vec()?;
            //
            //         let mut d2_ctx = PkeyCtx::new_id(Id::HKDF)?;
            //         d2_ctx.derive_init()?;
            //         d2_ctx.set_hkdf_salt(&[])?;
            //         d2_ctx.set_hkdf_md(Md::sha256())?;
            //         d2_ctx.add_hkdf_info(&[])?;
            //         d2_ctx.set_hkdf_key(derived_vec.as_slice())?;
            //         let mut aes_key_bytes: [u8; 16] = [0; 16];
            //         let _bytes = d2_ctx.derive(Some(&mut aes_key_bytes))?;
            //         self.aes_key_bytes = Some(aes_key_bytes.into());
            //
            //         Ok(Some(private_key.raw_public_key()?))
            //     } else {
            //         Err("No input provided".into())
            //     }
            // }
            _ => {
                error!("Unsupported algorithm: '{}'", self.algorithm);
                Err(TksError::ParameterError)
            }
        }
    }
    pub fn decrypt(&self, iv: &Vec<u8>, input: &Vec<u8>) -> Result<Vec<u8>, TksError> {
        trace!("Decrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            PLAIN => Ok(input.clone()),
            DH_AES => self
                .aes_key_bytes
                .as_ref()
                .ok_or_else(|| {
                    error!("Cannot decrypt: No key");
                    TksError::CryptoError
                })
                .map(|key| {
                    decrypt(Cipher::aes_128_cbc(), key, Some(iv), input).map_err(|e| {
                        error!("openssl error: {:?}", e);
                        TksError::CryptoError
                    })
                })?,
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err(TksError::ParameterError)
            }
        }
    }
    pub fn encrypt(&self, input: &Vec<u8>) -> Result<(Vec<u8>, Vec<u8>), Box<dyn error::Error>> {
        trace!("Encrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            PLAIN => Ok(([].to_vec(), input.clone())),
            DH_AES => {
                let iv = rand::random::<[u8; 16]>().to_vec();
                let input = input.clone();

                Ok((
                    iv.clone(),
                    encrypt(
                        Cipher::aes_128_cbc(),
                        &self.aes_key_bytes.as_ref().unwrap(),
                        Some(&iv),
                        &input,
                    )?,
                ))
            }
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
}
