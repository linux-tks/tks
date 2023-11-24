// Purpose: Main entry point for the program.
//
// Author: Valentin Rusu

mod dbus;

#[async_std::main]
async fn main() {
    println!("Starting server...");
    dbus::start_server().await.unwrap();
}
