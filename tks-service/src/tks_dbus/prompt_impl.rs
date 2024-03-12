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
use pinentry::{ConfirmationDialog, MessageDialog, PassphraseInput};
use rand::distributions::DistMap;
use secrecy::SecretString;
use std::collections::{BTreeMap as Map, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

pub trait TksPrompt {
    fn prompt(&mut self, _window_id: String) -> Result<bool, TksError>;
}

lazy_static! {
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

#[derive(Debug, Clone)]
pub struct PromptHandle {
    prompt_id: usize,
}

type PromptAction = dyn FnMut() -> Result<(), TksError> + Send;

pub struct PromptWithPinentry {
    prompt_id: usize,
    dialog: Box<dyn Dialog + Send>,
    text: String,
    on_confirmed: Box<PromptAction>,
    on_denied: Option<Box<PromptAction>>,
}

macro_rules! register_prompt {
    ($prompt:expr) => {{
        let handle = $prompt.get_dbus_handle();
        let path = handle.path().clone();
        PROMPTS
            .lock()
            .unwrap()
            .insert($prompt.prompt_id, Box::new($prompt));
        register_object!(register_org_freedesktop_secret_prompt, handle);
        path.into()
    }};
}

macro_rules! next_prompt_id {
    () => {{
        let mut counter = PROMPT_COUNTER.lock().unwrap();
        *counter += 1;
        *counter
    }};
}

impl PromptWithPinentry {
    pub fn new<D, F>(
        dialog: D,
        text: String,
        on_confirmed: F,
        on_denied: Option<F>,
    ) -> dbus::Path<'static>
    where
        D: Dialog + Send + 'static,
        F: FnMut() -> Result<(), TksError> + Send + 'static,
    {
        let prompt = PromptWithPinentry {
            prompt_id: next_prompt_id!(),
            text,
            dialog: Box::new(dialog),
            on_confirmed: Box::new(on_confirmed),
            on_denied: on_denied.map(|f| Box::new(f) as Box<PromptAction>),
        };
        register_prompt!(prompt)
    }
}

impl TksPrompt for PromptWithPinentry {
    fn prompt(&mut self, _window_id: String) -> Result<bool, TksError> {
        let dismissed: bool;
        match self.dialog.show(&self.text) {
            DialogResult::Confirmation(x) => {
                trace!("confirmation is {}", x);
                dismissed = !x;
                if x {
                    (self.on_confirmed)()?
                } else {
                    if let Some(f) = &mut self.on_denied {
                        f()?
                    }
                }
            }
            DialogResult::Secret(_x) => {
                trace!("passphrase entered");
                dismissed = false;
            }
            DialogResult::Unused => {
                trace!("Ingnoring message dialog result");
                dismissed = false;
            }
        }
        Ok(dismissed)
    }
}

pub struct TksFscryptPrompt {
    prompt_id: usize,
    coll_uuid: Uuid,
}

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

impl TksPrompt for TksFscryptPrompt {
    fn prompt(&mut self, _window_id: String) -> Result<bool, TksError> {
        Ok(false)
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

impl OrgFreedesktopSecretPrompt for PromptHandle {
    fn prompt(&mut self, window_id: String) -> Result<(), dbus::MethodErr> {
        trace!("prompt {}", window_id);

        let dismissed: bool;
        if let Some(prompt) = PROMPTS.lock().unwrap().get_mut(&self.prompt_id) {
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
                CROSSROADS.lock().unwrap().remove::<PromptHandle>(&prompt_path2);
            });
        });
        Ok(())
    }
    fn dismiss(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("dismiss {}", self.prompt_id);
        let prompt_path = self.path().clone();
        let prompt_id = self.prompt_id;
        let prompt_path2: dbus::Path<'static> = prompt_path.clone().into();
        tokio::spawn(async move {
            trace!("unregistering prompt {}", prompt_id);
            CROSSROADS.lock().unwrap().remove::<PromptHandle>(&prompt_path2);
        });
        Ok(())
    }
}

impl Dialog for MessageDialog<'_> {
    fn show(&self, text: &String) -> DialogResult {
        self.show_message(text).unwrap();
        DialogResult::Unused
    }
}
impl Dialog for PassphraseInput<'_> {
    fn show(&self, _text: &String) -> DialogResult {
        DialogResult::Secret(self.interact().unwrap())
    }
}
impl Dialog for ConfirmationDialog<'_> {
    fn show(&self, text: &String) -> DialogResult {
        DialogResult::Confirmation(self.confirm(text).unwrap())
    }
}

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
        register_prompt!(prompt)
    }
}

macro_rules! tks_prompt_from_path {
    ($path:expr) => {{}};
}

impl TksPrompt for TksPromptChain {
    fn prompt(&mut self, window_id: String) -> Result<bool, TksError> {
        let mut dismissed = false;
        for prompt_path in &self.prompts {
            // let tks_prompt = tks_prompt_from_path!(prompt_path).ok_or_else(|| {
            //     TksError::NotFound(Some(format!("Prompt not registered: {}", prompt_path)))
            // })?;
            let parts = prompt_path.split('/');
            let mut prompts = PROMPTS.lock().unwrap();
            let tks_prompt = match parts.count() {
                1 => None,
                5 => {
                    let id: usize = (prompt_path.split('/').nth(4).unwrap()).parse().unwrap();
                    prompts.get_mut(&id)
                }
                _ => {
                    debug!("Incorrect prompt path received {:?}", prompt_path);
                    None
                }
            }
            .ok_or_else(|| {
                TksError::NotFound(Some(format!("Prompt not registered: {}", prompt_path)))
            })?;
            dismissed |= tks_prompt.prompt(window_id.clone())?;
            if dismissed {
                break;
            }
        }
        Ok(dismissed)
    }
}
