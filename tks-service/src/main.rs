// Purpose: Main entry point for the program.
//
// Author: Valentin Rusu
#![feature(lazy_cell)]

extern crate log;
extern crate pretty_env_logger;

use std::future;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    tks_service::tks_dbus::start_server().await;
    future::pending::<()>().await;
    unreachable!();
}
