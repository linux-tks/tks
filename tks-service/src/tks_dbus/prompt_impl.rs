use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPrompt;
use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPromptCompleted;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus;
use dbus::arg;
use dbus::message::SignalArgs;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use pinentry::{ConfirmationDialog, MessageDialog, PassphraseInput};
use secrecy::SecretString;
use std::collections::BTreeMap as Map;
use std::sync::Arc;
use std::sync::Mutex;

lazy_static! {
    pub static ref PROMPTS: Arc<Mutex<Map<usize, PromptImpl>>> = Arc::new(Mutex::new(Map::new()));
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

pub struct PromptHandle {
    prompt_id: usize,
}

pub struct PromptImpl {
    prompt_id: usize,
    dialog: Box<dyn Dialog + Send>,
    text: String,
    on_confirmed: Box<dyn Fn() + Send>,
    on_denied: Option<Box<dyn Fn() + Send>>,
}

impl PromptImpl {
    pub fn new<D, F>(dialog: D, text: String, on_confirmed: F, on_denied: Option<F>) -> PromptHandle
    where
        D: Dialog + Send + 'static,
        F: Fn() + Send + 'static,
    {
        let prompt_id = {
            let mut counter = PROMPT_COUNTER.lock().unwrap();
            *counter += 1;
            *counter
        };
        let prompt = PromptImpl {
            prompt_id,
            text,
            dialog: Box::new(dialog),
            on_confirmed: Box::new(on_confirmed),
            on_denied: on_denied.map(|f| Box::new(f) as Box<dyn Fn() + Send>),
        };
        let handle = prompt.get_dbus_handle();
        PROMPTS.lock().unwrap().insert(prompt_id, prompt);
        handle
    }
    pub fn get_dbus_handle(&self) -> PromptHandle {
        PromptHandle {
            prompt_id: self.prompt_id,
        }
    }
}

impl DBusHandle for PromptHandle {
    fn path(&self) -> dbus::Path<'static> {
        dbus::Path::new(format!(
            "/org/freedesktop/secrets/prompt/{}",
            self.prompt_id
        ))
        .unwrap()
    }
}

impl OrgFreedesktopSecretPrompt for PromptHandle {
    fn prompt(&mut self, window_id: String) -> Result<(), dbus::MethodErr> {
        trace!("prompt {}", window_id);

        let dismissed: bool;
        if let Some(prompt) = PROMPTS.lock().unwrap().get_mut(&self.prompt_id) {
            match prompt.dialog.show(&prompt.text) {
                DialogResult::Confirmation(x) => {
                    trace!("confirmation is {}", x);
                    dismissed = !x;
                    if x {
                        (prompt.on_confirmed)();
                    } else {
                        if let Some(f) = &prompt.on_denied {
                            f();
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
        } else {
            error!("prompt not found");
            return Err(dbus::MethodErr::failed(
                "could not create confirmation dialog",
            ));
        };

        let prompt_path = self.path().clone();
        tokio::spawn(async move {
            debug!("sending prompt completed signal");
            MESSAGE_SENDER.lock().unwrap().send_message(
                OrgFreedesktopSecretPromptCompleted {
                    dismissed,
                    result: arg::Variant(Box::new((false, "".to_string()))),
                }
                .to_emit_message(&prompt_path),
            );
        });
        Ok(())
    }
    fn dismiss(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("dismiss");
        // TODO: figure a way to close pinentry dialog then finish implementation
        //
        // let prompt_path = self.path().clone();
        // tokio::spawn(async move {
        //     debug!("sending prompt dismissed signal");
        //     MESSAGE_SENDER.lock().unwrap().send_message(
        //         OrgFreedesktopSecretPromptCompleted {
        //             dismissed: false,
        //             result: arg::Variant(Box::new((true, "".to_string()))),
        //         }
        //         .to_emit_message(&prompt_path),
        //     );
        // });
        Err(dbus::MethodErr::failed("not implemented"))
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
