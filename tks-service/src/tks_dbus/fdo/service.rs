// This code was autogenerated with `dbus-codegen-rust -r`, see https://github.com/diwic/dbus-rs
use dbus;
#[allow(unused_imports)]
use dbus::arg;
use dbus_crossroads as crossroads;
use dbus_crossroads::{Context, PropContext};

pub trait OrgFreedesktopSecretService {
    fn open_session(
        &mut self,
        ctx: &mut Context,
        algorithm: String,
        input: arg::Variant<Box<dyn arg::RefArg + 'static>>,
    ) -> Result<
        (
            arg::Variant<Box<dyn arg::RefArg + 'static>>,
            dbus::Path<'static>,
        ),
        dbus::MethodErr,
    >;
    fn create_collection(
        &mut self,
        ctx: &mut Context,
        properties: arg::PropMap,
        alias: String,
    ) -> Result<(dbus::Path<'static>, dbus::Path<'static>), dbus::MethodErr>;
    fn search_items(
        &mut self,
        ctx: &mut Context,
        attributes: ::std::collections::HashMap<String, String>,
    ) -> Result<(Vec<dbus::Path<'static>>, Vec<dbus::Path<'static>>), dbus::MethodErr>;
    fn unlock(
        &mut self,
        ctx: &mut Context,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr>;
    fn lock(
        &mut self,
        ctx: &mut Context,
        objects: Vec<dbus::Path<'static>>,
    ) -> Result<(Vec<dbus::Path<'static>>, dbus::Path<'static>), dbus::MethodErr>;
    fn get_secrets(
        &mut self,
        ctx: &mut Context,
        items: Vec<dbus::Path<'static>>,
        session: dbus::Path<'static>,
    ) -> Result<
        ::std::collections::HashMap<
            dbus::Path<'static>,
            (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        >,
        dbus::MethodErr,
    >;
    fn read_alias(
        &mut self,
        ctx: &mut Context,
        name: String,
    ) -> Result<dbus::Path<'static>, dbus::MethodErr>;
    fn set_alias(
        &mut self,
        ctx: &mut Context,
        name: String,
        collection: dbus::Path<'static>,
    ) -> Result<(), dbus::MethodErr>;
    fn collections(
        &self,
        ctx: &mut PropContext,
    ) -> Result<Vec<dbus::Path<'static>>, dbus::MethodErr>;
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretServiceCollectionCreated {
    pub collection: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretServiceCollectionCreated {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.collection, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretServiceCollectionCreated {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretServiceCollectionCreated {
            collection: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretServiceCollectionCreated {
    const NAME: &'static str = "CollectionCreated";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Service";
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretServiceCollectionDeleted {
    pub collection: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretServiceCollectionDeleted {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.collection, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretServiceCollectionDeleted {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretServiceCollectionDeleted {
            collection: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretServiceCollectionDeleted {
    const NAME: &'static str = "CollectionDeleted";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Service";
}

#[derive(Debug)]
pub struct OrgFreedesktopSecretServiceCollectionChanged {
    pub collection: dbus::Path<'static>,
}

impl arg::AppendAll for OrgFreedesktopSecretServiceCollectionChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.collection, i);
    }
}

impl arg::ReadAll for OrgFreedesktopSecretServiceCollectionChanged {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgFreedesktopSecretServiceCollectionChanged {
            collection: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgFreedesktopSecretServiceCollectionChanged {
    const NAME: &'static str = "CollectionChanged";
    const INTERFACE: &'static str = "org.freedesktop.Secret.Service";
}

pub fn register_org_freedesktop_secret_service<T>(
    cr: &mut crossroads::Crossroads,
) -> crossroads::IfaceToken<T>
where
    T: OrgFreedesktopSecretService + Send + 'static,
{
    cr.register("org.freedesktop.Secret.Service", |b| {
        b.signal::<(dbus::Path<'static>,), _>("CollectionCreated", ("collection",));
        b.signal::<(dbus::Path<'static>,), _>("CollectionDeleted", ("collection",));
        b.signal::<(dbus::Path<'static>,), _>("CollectionChanged", ("collection",));
        b.method(
            "OpenSession",
            ("algorithm", "input"),
            ("output", "result"),
            |ctx, t: &mut T, (algorithm, input)| t.open_session(ctx, algorithm, input),
        );
        b.method(
            "CreateCollection",
            ("properties", "alias"),
            ("collection", "prompt"),
            |ctx, t: &mut T, (properties, alias)| t.create_collection(ctx, properties, alias),
        )
        .annotate("org.qtproject.QtDBus.QtTypeName.In0", "QVariantMap");
        b.method(
            "SearchItems",
            ("attributes",),
            ("unlocked", "locked"),
            |ctx, t: &mut T, (attributes,)| t.search_items(ctx, attributes),
        )
        .annotate("org.qtproject.QtDBus.QtTypeName.In0", "StrStrMap");
        b.method(
            "Unlock",
            ("objects",),
            ("unlocked", "prompt"),
            |ctx, t: &mut T, (objects,)| t.unlock(ctx, objects),
        );
        b.method(
            "Lock",
            ("objects",),
            ("locked", "Prompt"),
            |ctx, t: &mut T, (objects,)| t.lock(ctx, objects),
        );
        b.method(
            "GetSecrets",
            ("items", "session"),
            ("secrets",),
            |ctx, t: &mut T, (items, session)| t.get_secrets(ctx, items, session).map(|x| (x,)),
        )
        .annotate(
            "org.qtproject.QtDBus.QtTypeName.Out0",
            "FreedesktopSecretMap",
        );
        b.method(
            "ReadAlias",
            ("name",),
            ("collection",),
            |ctx, t: &mut T, (name,)| t.read_alias(ctx, name).map(|x| (x,)),
        );
        b.method(
            "SetAlias",
            ("name", "collection"),
            (),
            |ctx, t: &mut T, (name, collection)| t.set_alias(ctx, name, collection),
        );
        b.property::<Vec<dbus::Path<'static>>, _>("Collections")
            .get(|ctx, t| t.collections(ctx));
    })
}
