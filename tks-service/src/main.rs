// Purpose: Main entry point for the program.
//
// Author: Valentin Rusu
#![feature(lazy_cell)]

mod tks_dbus;
extern crate log;
extern crate pretty_env_logger;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    tks_dbus::start_server().await;
}
