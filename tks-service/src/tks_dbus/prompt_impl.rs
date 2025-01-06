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
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};
use pinentry::{ConfirmationDialog, MessageDialog};
use scopeguard::defer;
use secrecy::SecretString;
use std::cell::RefCell;
use std::collections::{BTreeMap as Map, VecDeque};
use std::ffi::OsString;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PromptHandle {
    prompt_id: usize,
}

pub trait TksPrompt {
    fn prompt(&self, _window_id: String) -> Result<(bool, Option<PromptChainPaths>), TksError>;
    fn dismiss(&self) -> Result<(), TksError>;
}

lazy_static! {
    // This is the list of the DBus-registered prompts, that are yet to be invoked
    // by the client applications
    pub static ref PROMPTS: Arc<ReentrantMutex<RefCell<Map<usize, Box<dyn TksPrompt + Send>>>>> =
        Arc::new(ReentrantMutex::new(RefCell::new(Map::new())));
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
            .deref()
            .borrow_mut()
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

trait ConfirmedAction {
    /// to keep logic consistent with the prompts, this returns `true` when action is dismissed
    fn confirmed(&self) -> bool;
}

#[derive(Clone, Debug)]
pub enum ConfirmationMessageActionParam {
    ConfirmNewClient(OsString)
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
    ConfirmationMessage(
        // ConfirmationDialog::with_ok(1).with_cancel(2).confirm(3)
        String,                         // String on the OK button
        String,                         // String on the Cancel button
        String,                         // Confirmation message
        ConfirmationMessageActionParam,
        fn(&ConfirmationMessageActionParam) -> Result<bool, TksError>,
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

    // returns true if the dialog has been dismissed, false otherwise
    pub fn perform(&self) -> Result<bool, TksError> {
        match &self.dialog {
            PromptDialog::PromptMessage(ok, msg) => {
                if let Some(mut d) = MessageDialog::with_default_binary() {
                    d.with_ok(ok).show_message(msg).unwrap();
                    Ok(false)
                } else {
                    Err(TksError::NoPinentryBinaryFound)
                }
            }
            PromptDialog::PassphraseInput(desc, prompt, confirmation, mismatch, action) => {
                if let Some(mut d) = pinentry::PassphraseInput::with_default_binary() {
                    d.required("Password is required".into())
                        .with_prompt(prompt.as_str())
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
            PromptDialog::ConfirmationMessage(yes, no, confirmation, action_param, action) => {
                if let Some(mut input) = ConfirmationDialog::with_default_binary() {
                    let dismissed = !input.with_ok(yes).with_cancel(no).confirm(confirmation)?;
                    if (dismissed) {
                        trace!("User dismissed confirmation '{}", confirmation);
                        Ok(dismissed)
                    } else {
                        Ok(action(action_param)?)
                    }
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
        // TODO users might forget to use prompts, so attach a timer on each and self destruct after several minutes
        Ok(register_prompt!(prompt).into())
    }
}

impl TksPrompt for PromptWithPinentry {
    /// returns `true` when dismissed
    fn prompt(&self, _window_id: String) -> Result<(bool, Option<PromptChainPaths>), TksError> {
        Ok((self.action.perform()?, None))
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

        let dismissed: bool = true; // errors effectively dismiss us
        let mut chain_paths: Option<PromptChainPaths> = None;
        let prompt_path = self.path().clone();
        let prompt_id = self.prompt_id;
        let mut guard = scopeguard::guard((dismissed, chain_paths), |(dismissed, chain_paths)| {
            // ensure we unregister the prompt once interaction has been done, but also in any case of error
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
                PROMPTS.lock().deref().borrow_mut().remove(&prompt_id);
                tokio::spawn(async move {
                    trace!("unregistering prompt {}", prompt_id);
                    CROSSROADS
                        .lock()
                        .unwrap()
                        .remove::<PromptHandle>(&prompt_path2);
                });
                if let Some(paths) = chain_paths {
                    for path in paths {
                        tokio::spawn(async move {
                            trace!("unregistering prompt {}", prompt_id);
                            CROSSROADS.lock().unwrap().remove::<PromptHandle>(&path);
                        });
                    }
                }
            });
        });

        if let Some(prompt) = PROMPTS.lock().deref().borrow().get(&self.prompt_id) {
            *guard = prompt.prompt(window_id)?;
        } else {
            error!("prompt not found");
            return Err(dbus::MethodErr::failed(
                "could not create confirmation dialog",
            ));
        };

        Ok(())
    }
    fn dismiss(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("dismiss {}", self.prompt_id);
        if let Some(prompt) = PROMPTS.lock().deref().borrow().get(&self.prompt_id) {
            prompt.dismiss()?
        } else {
            error!("prompt not found");
            return Err(dbus::MethodErr::failed("could not dismiss unknown prompt"));
        };

        let prompt_path = self.path().clone();
        let prompt_id = self.prompt_id;
        let prompt_path2: dbus::Path<'static> = prompt_path.clone().into();
        tokio::spawn(async move {
            trace!("sending prompt completed signal");
            let prompt_path2: dbus::Path<'static> = prompt_path.clone().into();
            MESSAGE_SENDER.lock().unwrap().send_message(
                OrgFreedesktopSecretPromptCompleted {
                    dismissed: true,
                    result: arg::Variant(Box::new((false, "".to_string()))),
                }
                .to_emit_message(&prompt_path.into()),
            );
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

type PromptChainPaths = VecDeque<dbus::Path<'static>>;
#[derive(Clone)]
pub struct TksPromptChain {
    prompts: PromptChainPaths,
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

    fn invoke_prompts(
        &self,
        window_id: Option<String>,
        dismissed: bool,
    ) -> Result<(bool, Option<PromptChainPaths>), TksError> {
        let mut dismissed = dismissed;
        assert!(dismissed || window_id.is_some());
        for prompt_path in &self.prompts {
            let mut parts = prompt_path.split('/');
            match parts.clone().count() {
                6 => {
                    let ids = parts.nth(5).unwrap();
                    let id: usize = ids.parse().unwrap();
                    dismissed |= PROMPTS.lock().deref().borrow().get(&id).map_or_else(
                        || {
                            Err(TksError::NotFound(Some(format!(
                                "Prompt not registered: {}",
                                prompt_path
                            ))))
                        },
                        |p| {
                            if dismissed {
                                p.dismiss()?;
                                Ok(dismissed)
                            } else {
                                let (dismissed, _) = p.prompt(window_id.clone().unwrap())?;
                                Ok(dismissed)
                            }
                        },
                    )?;
                }
                n => {
                    debug!(
                        "Incorrect prompt path received {:?} which as {} parts",
                        prompt_path, n
                    );
                    return Err(TksError::NotFound(Some(prompt_path.to_string())));
                }
            }
        }
        // FIXME in case of premature error, caller no longer get the prompts to be unregistered so the subordinate
        // prompts won't get unregistered
        Ok((dismissed, Some(self.prompts.clone())))
    }
}

macro_rules! tks_prompt_from_path {
    ($path:expr) => {{}};
}

impl TksPrompt for TksPromptChain {
    fn prompt(&self, window_id: String) -> Result<(bool, Option<PromptChainPaths>), TksError> {
        self.invoke_prompts(Some(window_id), false)
    }

    fn dismiss(&self) -> Result<(), TksError> {
        debug!("dismiss the prompt chain");
        self.invoke_prompts(None, true).map(|_| {})
    }
}
