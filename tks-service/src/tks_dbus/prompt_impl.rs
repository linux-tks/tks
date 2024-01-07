use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPrompt;
use crate::tks_dbus::fdo::prompt::OrgFreedesktopSecretPromptCompleted;
use crate::tks_dbus::DBusHandle;
use crate::tks_dbus::MESSAGE_SENDER;
use dbus;
use dbus::arg;
use dbus::message::SignalArgs;
use log::{debug, error, trace};

pub struct PromptHandle {}

pub struct PromptImpl {}

impl PromptImpl {
    pub fn new() -> PromptImpl {
        PromptImpl {}
    }
    pub fn get_dbus_handle(&self) -> PromptHandle {
        PromptHandle {}
    }
}

impl DBusHandle for PromptHandle {
    fn path(&self) -> dbus::Path<'static> {
        dbus::Path::new("/org/freedesktop/secrets/prompt/1").unwrap()
    }
}

impl OrgFreedesktopSecretPrompt for PromptHandle {
    fn prompt(&mut self, window_id: String) -> Result<(), dbus::MethodErr> {
        trace!("prompt {}", window_id);
        // FIXME: immediately send a completed signal, but this should be a real prompt
        let prompt_path = self.path().clone();
        tokio::spawn(async move {
            debug!("sending prompt completed signal");
            MESSAGE_SENDER.lock().unwrap().send_message(
                OrgFreedesktopSecretPromptCompleted {
                    dismissed: false,
                    result: arg::Variant(Box::new((false, "".to_string()))),
                }
                .to_emit_message(&prompt_path),
            );
        });
        Ok(())
    }
    fn dismiss(&mut self) -> Result<(), dbus::MethodErr> {
        trace!("dismiss");
        let prompt_path = self.path().clone();
        tokio::spawn(async move {
            debug!("sending prompt dismissed signal");
            MESSAGE_SENDER.lock().unwrap().send_message(
                OrgFreedesktopSecretPromptCompleted {
                    dismissed: true,
                    result: arg::Variant(Box::new((true, "".to_string()))),
                }
                .to_emit_message(&prompt_path),
            );
        });
        Ok(())
    }
}
