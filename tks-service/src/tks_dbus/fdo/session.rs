// This code was autogenerated with `dbus-codegen-rust -r`, see https://github.com/diwic/dbus-rs
use dbus;
#[allow(unused_imports)]
use dbus::arg;
use dbus_crossroads as crossroads;
use dbus_crossroads::Context;

pub trait OrgFreedesktopSecretSession {
    fn close(&mut self, ctx: &mut Context) -> Result<(), dbus::MethodErr>;
}

pub fn register_org_freedesktop_secret_session<T>(
    cr: &mut crossroads::Crossroads,
) -> crossroads::IfaceToken<T>
where
    T: OrgFreedesktopSecretSession + Send + 'static,
{
    cr.register("org.freedesktop.Secret.Session", |b| {
        b.method("Close", (), (), |ctx, t: &mut T, ()| t.close(ctx));
    })
}
