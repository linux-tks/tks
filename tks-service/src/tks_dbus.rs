use dbus::blocking::Connection;
use dbus_crossroads::Crossroads;
use std::error::Error;

use crate::tks_dbus::service_impl::register_service;
mod fdo;
mod service_impl;
mod session_impl;

pub fn start_server() -> Result<(), Box<dyn Error>> {
    let connection = Connection::new_session()?;
    connection.request_name("org.freedesktop.secrets", false, true, false)?;

    let mut crossroads = Crossroads::new();

    register_service(&mut crossroads);

    trace!("Start serving");
    crossroads.serve(&connection)?;
    error!("Crossroads has stopped serving");
    unreachable!();
}
