use crate::tks_dbus::fdo::session::OrgFreedesktopSecretSession;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::CROSSROADS;
use hmac::{Hmac, Mac, SimpleHmac};
use hmac_sha256::HKDF;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use num_bigint::RandBigInt;
use num_bigint::{BigInt, BigUint, ToBigInt};
use openssl::aes::{aes_ige, AesKey};
use openssl::derive::Deriver;
use openssl::md::Md;
use openssl::pkey::Id;
use openssl::pkey::PKey;
use openssl::pkey_ctx::PkeyCtx;
use openssl::symm::{decrypt, Cipher};
use ring::{agreement, rand};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::{error, panic, ptr};
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
    fn path(&self) -> dbus::Path<'static> {
        "/org/freedesktop/secrets/session".into()
    }
}

const DH_AES: &'static str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";
const X25519: &'static str = "x25519";
const PLAIN: &'static str = "plain";

// bigint implementation of DH_PRIME
// const DH_PRIME: [u8; 128] = [
//     0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xC9, 0x0F, 0xDA, 0xA2, 0x21, 0x68, 0xC2, 0x34,
//     0xC4, 0xC6, 0x62, 0x8B, 0x80, 0xDC, 0x1C, 0xD1, 0x29, 0x02, 0x4E, 0x08, 0x8A, 0x67, 0xCC, 0x74,
//     0x02, 0x0B, 0xBE, 0xA6, 0x3B, 0x13, 0x9B, 0x22, 0x51, 0x4A, 0x08, 0x79, 0x8E, 0x34, 0x04, 0xDD,
//     0xEF, 0x95, 0x19, 0xB3, 0xCD, 0x3A, 0x43, 0x1B, 0x30, 0x2B, 0x0A, 0x6D, 0xF2, 0x5F, 0x14, 0x37,
//     0x4F, 0xE1, 0x35, 0x6D, 0x6D, 0x51, 0xC2, 0x45, 0xE4, 0x85, 0xB5, 0x76, 0x62, 0x5E, 0x7E, 0xC6,
//     0xF4, 0x4C, 0x42, 0xE9, 0xA6, 0x37, 0xED, 0x6B, 0x0B, 0xFF, 0x5C, 0xB6, 0xF4, 0x06, 0xB7, 0xED,
//     0xEE, 0x38, 0x6B, 0xFB, 0x5A, 0x89, 0x9F, 0xA5, 0xAE, 0x9F, 0x24, 0x11, 0x7C, 0x4B, 0x1F, 0xE6,
//     0x49, 0x28, 0x66, 0x51, 0xEC, 0xE6, 0x53, 0x81, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
// ];

enum TksError {
    ParameterError,
    CryptoError,
    IOError(std::io::Error),
}

impl From<ring::error::Unspecified> for TksError {
    fn from(_: ring::error::Unspecified) -> Self {
        TksError::CryptoError
    }
}

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

            DH_AES =>
            // bigint implementation of DH_AES
            {
                if let Some(input) = input {
                    //     let mut rng = rand::thread_rng();
                    //     let private_key = rng.gen_biguint(1024);
                    //     let prime = BigUint::from_bytes_be(&DH_PRIME);
                    //     let pub_key = BigUint::parse_bytes(b"2", 10)
                    //         .unwrap()
                    //         .modpow(&private_key, &prime);
                    //
                    //     let client_pub_key = BigUint::from_bytes_be(&input);
                    //     let mut common_secret =
                    //         client_pub_key.modpow(&private_key, &prime).to_bytes_be();
                    //     let common_secret = match common_secret.len().cmp(&16) {
                    //         Ordering::Less => {
                    //             let mut x = vec![0u8; 128 - common_secret.len()];
                    //             x.append(&mut common_secret);
                    //             x
                    //         }
                    //         Ordering::Greater => {
                    //             common_secret.truncate(128);
                    //             common_secret
                    //         }
                    //         Ordering::Equal => common_secret,
                    //     };
                    //
                    //     let salt: [u8; 32] = [0; 32];
                    //     let iv: [u8; 1] = [0; 1];
                    //     let mut secret_key: [u8; 16] = [0; 16];
                    //     HKDF::expand(&mut secret_key, HKDF::extract(&salt, &common_secret), &iv);
                    //     debug!("secret key: {:?}", secret_key);
                    //     self.aes_key_bytes = Some(secret_key.into());

                    let rng = rand::SystemRandom::new();
                    let private_key =
                        agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)?;
                    let public_key = private_key.compute_public_key()?;
                    let peer_public_key =
                        agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, input);
                    let shared_secret = agreement::agree_ephemeral(
                        private_key,
                        &peer_public_key,
                        |_key_material| Ok(()),
                    )?;
                    Ok(Some(public_key.as_ref().to_vec()))
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
    pub fn decrypt(&self, iv: &Vec<u8>, input: &Vec<u8>) -> Result<Vec<u8>, Box<dyn error::Error>> {
        trace!("Decrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            PLAIN => Ok(input.clone()),
            DH_AES => {
                // openssl decrypt
                // if let Some(key) = self.aes_key_bytes.as_ref() {
                //     let cipher = Cipher::aes_128_cbc();
                //     Ok(decrypt(cipher, key, Some(iv), input)?)
                // } else {
                //     error!("No key");
                //     Err("No key".into())
                // }
            }
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
    pub fn encrypt(&self, input: &Vec<u8>) -> Result<Vec<u8>, Box<dyn error::Error>> {
        trace!("Encrypting secret for session {}", self.id);
        match self.algorithm.as_str() {
            PLAIN => Ok(input.clone()),
            DH_AES => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
            _ => {
                error!("Unsupported algorithm: {}", self.algorithm);
                Err("Unsupported algorithm".into())
            }
        }
    }
}
