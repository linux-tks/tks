// Purpose: Main entry point for the program.
//
// Author: Valentin Rusu

mod tks_dbus;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

fn main() {
    pretty_env_logger::init();
    trace!("Starting server...");
    match tks_dbus::start_server() {
        Ok(_) => info!("Server started successfully"),
        Err(e) => error!("Server failed to start: {}", e),
    }
}
