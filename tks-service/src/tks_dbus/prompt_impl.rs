use crate::register_object;
use crate::tks_dbus::fdo::prompt::register_org_freedesktop_secret_prompt;
use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPrompt;
use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPromptCompleted;
use crate::tks_dbus::DBusHandlePath::SinglePath;
use crate::tks_dbus::CROSSROADS;
use crate::tks_dbus::MESSAGE_SENDER;
use crate::tks_dbus::{DBusHandle, DBusHandlePath};
use crate::tks_error::TksError;
use dbus;
use dbus::message::SignalArgs;
use dbus::{arg, Path};
use lazy_static::lazy_static;
use log::{debug, error, trace};
use pinentry::{ConfirmationDialog, MessageDialog};
use secrecy::SecretString;
use std::collections::{BTreeMap as Map, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PromptHandle {
    prompt_id: usize,
}

pub trait TksPrompt {
    fn prompt(&self, _window_id: String) -> Result<bool, TksError>;
    fn dismiss(&self) -> Result<(), TksError>;
}

lazy_static! {
    // This is the list of the DBus-registered prompts, that are yet to be invoked
    // by the client applications
    pub static ref PROMPTS: Arc<Mutex<Map<usize, Box<dyn TksPrompt + Send>>>> =
        Arc::new(Mutex::new(Map::new()));
    pub static ref PROMPT_COUNTER: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
}

pub enum DialogResult {
    Secret(SecretString),
    Confirmation(bool),
    Unused,
}

pub trait Dialog {
    fn show(&self, text: &String) -> DialogResult;
}

macro_rules! register_prompt {
    ($prompt:expr) => {{
        let handle = $prompt.get_dbus_handle();
        let path = handle.path().clone();
        PROMPTS
            .lock()
            .unwrap()
            .insert($prompt.prompt_id, Box::new($prompt.clone()));
        register_object!(register_org_freedesktop_secret_prompt, handle);
        path
    }};
}

macro_rules! next_prompt_id {
    () => {{
        let mut counter = PROMPT_COUNTER.lock().unwrap();
        *counter += 1;
        *counter
    }};
}

#[derive(Clone, Debug)]
pub enum PromptDialog {
    PromptMessage(String, String), //  MessageDialog.with_ok(1).show_message(2)
    PassphraseInput(
        String,                                     // description
        String,                                     // prompt
        Option<String>,                             // confirmation
        Option<String>,                             // mismatch message
        fn(SecretString) -> Result<bool, TksError>, // action if user confirms dialog
    ),
}
#[derive(Clone, Debug)]
pub struct PromptAction {
    pub(crate) dialog: PromptDialog,
}

impl PromptAction {
    pub(crate) fn dismiss(&self) -> Result<(), TksError> {
        debug!("PromptAction dismiss");
        Ok(())
    }
}

impl PromptAction {
    //! returns true if the dialog has been dismissed, false otherwise
    pub fn perform(&self) -> Result<bool, TksError> {
        match &self.dialog {
            PromptDialog::PromptMessage(ok, msg) => {
                if let Some(mut d) = MessageDialog::with_default_binary() {
                    d.with_ok(ok).show_message(msg).unwrap();
                    Ok(true)
                } else {
                    Err(TksError::NoPinentryBinaryFound)
                }
            }
            PromptDialog::PassphraseInput(desc, prompt, confirmation, mismatch, action) => {
                if let Some(mut d) = pinentry::PassphraseInput::with_default_binary() {
                    d.with_prompt(prompt.as_str())
                        .with_description(desc.as_str());
                    let mis: String;
                    if let Some(conf) = confirmation {
                        mis = mismatch.clone().unwrap();
                        d.with_confirmation(conf.as_str(), mis.as_str());
                    }
                    let s = d.interact()?;
                    action(s)
                } else {
                    Err(TksError::NoPinentryBinaryFound)
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct PromptWithPinentry {
    prompt_id: usize,
    action: PromptAction,
}

impl PromptWithPinentry {
    pub fn new(action: PromptAction) -> Result<dbus::Path<'static>, TksError> {
        let prompt = PromptWithPinentry {
            prompt_id: next_prompt_id!(),
            action: action.clone(),
        };
        Ok(register_prompt!(prompt).into())
    }
}

impl TksPrompt for PromptWithPinentry {
    fn prompt(&self, _window_id: String) -> Result<bool, TksError> {
        Ok(self.action.perform()?)
    }

    fn dismiss(&self) -> Result<(), TksError> {
        self.action.dismiss()
    }
}

#[cfg(feature = "fscrypt")]
pub struct TksFscryptPrompt {
    prompt_id: usize,
    coll_uuid: Uuid,
}

#[cfg(feature = "fscrypt")]
impl TksFscryptPrompt {
    pub fn new(coll_uuid: &Uuid) -> dbus::Path<'static> {
        trace!("new");
        let prompt = TksFscryptPrompt {
            prompt_id: next_prompt_id!(),
            coll_uuid: coll_uuid.clone(),
        };
        register_prompt!(prompt)
    }
}

#[cfg(feature = "fscrypt")]
impl TksPrompt for TksFscryptPrompt {
    fn prompt(&self, _window_id: String) -> Result<bool, TksError> {
        Ok(false)
    }

    fn dismiss(&self) -> Result<(), TksError> {
        todo!()
    }
}

trait GetPromptDbusHandle {
    fn get_dbus_handle(&self) -> PromptHandle;
}

macro_rules! prompt_handle {
    ($prompt:expr) => {{
        PromptHandle {
            prompt_id: $prompt.prompt_id,
        }
    }};
}
#[cfg(feature = "fscrypt")]
impl GetPromptDbusHandle for TksFscryptPrompt {
    fn get_dbus_handle(&self) -> PromptHandle {
        prompt_handle!(self)
    }
}

impl GetPromptDbusHandle for PromptWithPinentry {
    fn get_dbus_handle(&self) -> PromptHandle {
        prompt_handle!(self)
    }
}
impl GetPromptDbusHandle for TksPromptChain {
    fn get_dbus_handle(&self) -> PromptHandle {
        prompt_handle!(self)
    }
}
impl DBusHandle for PromptHandle {
    fn path(&self) -> DBusHandlePath {
        SinglePath(format!("/org/freedesktop/secrets/prompt/{}", self.prompt_id).into())
    }
}
impl DBusHandle for TksPromptChain {
    fn path(&self) -> DBusHandlePath {
        SinglePath(format!("/org/freedesktop/secrets/prompt/{}", self.prompt_id).into())
    }
}

impl From<DBusHandlePath> for Result<dbus::Path<'_>, TksError> {
    fn from(value: DBusHandlePath) -> Self {
        Ok(Path::from(value))
    }
}
impl OrgFreedesktopSecretPrompt for PromptHandle {
    fn prompt(&mut self, window_id: String) -> Result<(), dbus::MethodErr> {
        trace!("prompt {}", window_id);

        let dismissed: bool;
        if let Some(prompt) = PROMPTS.lock().unwrap().get(&self.prompt_id) {
            dismissed = prompt.prompt(window_id)?;
        } else {
            error!("prompt not found");
            return Err(dbus::MethodErr::failed(
                "could not create confirmation dialog",
            ));
        };

        let prompt_path = self.path().clone();
        let prompt_id = self.prompt_id;
        tokio::spawn(async move {
            trace!("sending prompt completed signal");
            let prompt_path2: dbus::Path<'static> = prompt_path.clone().into();
            MESSAGE_SENDER.lock().unwrap().send_message(
                OrgFreedesktopSecretPromptCompleted {
                    dismissed,
                    result: arg::Variant(Box::new((false, "".to_string()))),
                }
                .to_emit_message(&prompt_path.into()),
            );
            PROMPTS.lock().unwrap().remove(&prompt_id);
            tokio::spawn(async move {
                trace!("unregistering prompt {}", prompt_id);
                CROSSROADS
                    .lock()
                    .unwrap()
                    .remove::<PromptHandle>(&prompt_path2);
            });
        });
        Ok(())
    }
    fn dismiss(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("dismiss {}", self.prompt_id);
        if let Some(prompt) = PROMPTS.lock().unwrap().get(&self.prompt_id) {
            prompt.dismiss()?
        } else {
            error!("prompt not found");
            return Err(dbus::MethodErr::failed("could not dismiss unknown prompt"));
        };

        let prompt_path = self.path().clone();
        let prompt_id = self.prompt_id;
        let prompt_path2: dbus::Path<'static> = prompt_path.clone().into();
        tokio::spawn(async move {
            trace!("unregistering prompt {}", prompt_id);
            CROSSROADS
                .lock()
                .unwrap()
                .remove::<PromptHandle>(&prompt_path2);
        });
        Ok(())
    }
}

impl Dialog for &mut MessageDialog<'_> {
    fn show(&self, text: &String) -> DialogResult {
        self.show_message(text).unwrap();
        DialogResult::Unused
    }
}
impl Dialog for pinentry::PassphraseInput<'_> {
    fn show(&self, _text: &String) -> DialogResult {
        DialogResult::Secret(self.interact().unwrap())
    }
}
impl Dialog for ConfirmationDialog<'_> {
    fn show(&self, text: &String) -> DialogResult {
        DialogResult::Confirmation(self.confirm(text).unwrap())
    }
}

#[derive(Clone)]
pub struct TksPromptChain {
    prompts: VecDeque<dbus::Path<'static>>,
    prompt_id: usize,
}

impl TksPromptChain {
    pub fn new(prompts: VecDeque<Path<'static>>) -> dbus::Path<'static> {
        let prompt = TksPromptChain {
            prompts,
            prompt_id: next_prompt_id!(),
        };
        register_prompt!(prompt).into()
    }

    fn invoke_prompts(&self, window_id: Option<String>, dismissed: bool) -> Result<bool, TksError> {
        let mut dismissed = dismissed;
        assert!(dismissed || window_id.is_some());
        for prompt_path in &self.prompts {
            let parts = prompt_path.split('/');
            match parts.count() {
                5 => {
                    let id: usize = (prompt_path.split('/').nth(4).unwrap()).parse().unwrap();
                    dismissed |= PROMPTS.lock().unwrap().get(&id).map_or_else(
                        || {
                            Err(TksError::NotFound(Some(format!(
                                "Prompt not registered: {}",
                                prompt_path
                            ))))
                        },
                        |p| {
                            if dismissed {
                                p.dismiss();
                                Ok(dismissed)
                            } else {
                                Ok(p.prompt(window_id.clone().unwrap())?)
                            }
                        },
                    )?;
                }
                _ => {
                    debug!("Incorrect prompt path received {:?}", prompt_path);
                    return Err(TksError::NotFound(Some(prompt_path.to_string())));
                }
            }
        }
        Ok(dismissed)
    }
}

macro_rules! tks_prompt_from_path {
    ($path:expr) => {{}};
}

impl TksPrompt for TksPromptChain {
    fn prompt(&self, window_id: String) -> Result<bool, TksError> {
        self.invoke_prompts(Some(window_id), false)
    }

    fn dismiss(&self) -> Result<(), TksError> {
        debug!("dismiss the prompt chain");
        self.invoke_prompts(None, true).map(|_| {})
    }
}
