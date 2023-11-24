use dbus::nonblock::Connection;
use dbus_crossroads::Context;
use dbus_crossroads::Crossroads;
use std::error::Error;

pub fn start_server() -> Result<(), Box<dyn Error>> {
    let connection = Connection::new_session()?;
    connection
        .request_name("org.tks.server", false, true, false)
        .await?;

    let mut crossroads = Crossroads::new();

    crossroads.insert(
        "org.tks.server",
        &[("org.tks.server", "Hello", (), ())],
        |_, _, _: &mut Context, _: ()| {
            println!("Hello from server");
            Ok(())
        },
    );

    crossroads.serve(&connection)?;
    unreachable!();
}
