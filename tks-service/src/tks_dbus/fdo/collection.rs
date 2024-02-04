// This code was autogenerated with `dbus-codegen-rust -r`, see https://github.com/diwic/dbus-rs
use dbus;
use dbus::arg;
use dbus_crossroads as crossroads;
use dbus_crossroads::Context;

pub trait OrgFreedesktopSecretCollection {
    fn delete(&mut self) -> Result<dbus::Path<'static>, dbus::MethodErr>;
    fn search_items(
        &mut self,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr>;
    fn create_item(
        &mut self,
        properties: arg::PropMap,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        replace: bool,
        ctx: &mut Context,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr>;
    fn items(&self) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr>;
    fn label(&self) -> Result<String, dbus::MethodErr>;
    fn set_label(&self, value: String) -> Result<(), dbus::MethodErr>;
    fn locked(&self) -> Result<bool, dbus::MethodErr>;
    fn created(&self) -> Result<u64, dbus::MethodErr>;
    fn modified(&self) -> Result<u64, dbus::MethodErr>;
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretCollectionItemCreated {
    pub item: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretCollectionItemCreated {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.item, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretCollectionItemCreated {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretCollectionItemCreated { item: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretCollectionItemCreated {
    const NAME: &'static str = "ItemCreated";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Collection";
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretCollectionItemDeleted {
    pub item: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretCollectionItemDeleted {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.item, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretCollectionItemDeleted {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretCollectionItemDeleted { item: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretCollectionItemDeleted {
    const NAME: &'static str = "ItemDeleted";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Collection";
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretCollectionItemChanged {
    pub item: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretCollectionItemChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.item, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretCollectionItemChanged {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretCollectionItemChanged { item: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretCollectionItemChanged {
    const NAME: &'static str = "ItemChanged";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Collection";
}

pub fn register_org_freedesktop_secret_collection<T>(
    cr: &mut crossroads::Crossroads,
) -> crossroads::IfaceToken<T>
where
    T: OrgFreedesktopSecretCollection + Send + 'static,
{
    cr.register("org.freedesktop.Secret.Collection", |b| {
        b.signal::<(dbus::Path<'static>,), _>("ItemCreated", ("item",));
        b.signal::<(dbus::Path<'static>,), _>("ItemDeleted", ("item",));
        b.signal::<(dbus::Path<'static>,), _>("ItemChanged", ("item",));
        b.method("Delete", (), ("prompt",), |_, t: &mut T, ()| {
            t.delete().map(|x| (x,))
        });
        b.method(
            "SearchItems",
            ("attributes",),
            ("results",),
            |_, t: &mut T, (attributes,)| t.search_items(attributes).map(|x| (x,)),
        )
        .annotate("org.qtproject.QtDBus.QtTypeName.In0", "StrStrMap");
        b.method(
            "CreateItem",
            ("properties", "secret", "replace"),
            ("item", "prompt"),
            |ctx, t: &mut T, (properties, secret, replace)| {
                t.create_item(properties, secret, replace, ctx)
            },
        )
        .annotate("org.qtproject.QtDBus.QtTypeName.In1", "FreedesktopSecret")
        .annotate("org.qtproject.QtDBus.QtTypeName.In0", "PropertiesMap");
        b.property::<Vec<dbus::Path<'static>>, _>("Items")
            .get(|_, t| t.items());
        b.property::<String, _>("Label")
            .get(|_, t| t.label())
            .set(|_, t, value| t.set_label(value).map(|_| None));
        b.property::<bool, _>("Locked").get(|_, t| t.locked());
        b.property::<u64, _>("Created").get(|_, t| t.created());
        b.property::<u64, _>("Modified").get(|_, t| t.modified());
    })
}
