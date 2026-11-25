use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;
use zbus::names::InterfaceName;
use zbus::{Connection, ObjectPath, ObjectServer, Variant};

use super::advertisement::{Advertisement, AdvertisementConfig, AdvertisementGuard};
use super::characteristic::{CharacteristicHandle, CharacteristicInner, Value};
use super::service::{Service, ServiceBuilder};

/// A service or characteristic exported by an [`Application`].
///
/// Services use the key `(service_uuid, Uuid::nil())` and have no `inner`.
struct ExportedInterface {
    key: (Uuid, Uuid),
    path: ObjectPath<'static>,
    /// Type-erased `Arc<CharacteristicInner<T>>`; `None` for services.
    inner: Option<Arc<dyn Any + Send + Sync>>,
}

/// A GATT application registered with BlueZ.
///
/// Created with [`Application::builder`]. Dropping it does not unregister it;
/// call [`Self::unregister`].
pub struct Application {
    pub(super) conn: Connection,
    pub(super) path: Box<str>,
    /// BlueZ adapter object path, e.g. `/org/bluez/hci0`.
    pub(super) adapter: Box<str>,
    /// Sorted by `key`, so characteristics can be found by binary search.
    interfaces: Box<[ExportedInterface]>,
}

impl Application {
    /// Returns a builder for an application exported at the D-Bus object `path`.
    pub fn builder(path: &str) -> ApplicationBuilder {
        ApplicationBuilder {
            path: path.into(),
            services: Vec::new(),
        }
    }

    /// Registers an LE advertisement for this application.
    ///
    /// # Errors
    ///
    /// Returns an error if BlueZ rejects the advertisement.
    pub fn advertise(
        &self,
        conf: AdvertisementConfig<'_>,
    ) -> impl Future<Output = zbus::Result<AdvertisementGuard<'_>>> {
        Advertisement::register(self, conf)
    }

    /// Returns a handle to the characteristic `chcr` of `service`.
    ///
    /// Returns `None` if it does not exist or its value type is not `T`.
    pub fn get_characteristic<T: Value>(
        &self,
        service: Uuid,
        chcr: Uuid,
    ) -> Option<CharacteristicHandle<T>> {
        use zbus::object_server::SignalEmitter;

        let idx = self
            .interfaces
            .binary_search_by_key(&(service, chcr), |i| i.key)
            .ok()?;

        let iface = &self.interfaces[idx];
        let inner = Arc::clone(iface.inner.as_ref()?);
        let inner = Arc::downcast::<CharacteristicInner<T>>(inner).ok()?;
        let emitter = SignalEmitter::from_parts(self.conn.clone(), iface.path.clone());
        Some(CharacteristicHandle::new(inner, emitter))
    }

    /// Unregisters the application from BlueZ and removes it from D-Bus.
    ///
    /// Cleanup always runs to completion; the first error is returned.
    ///
    /// Unregister any [`AdvertisementGuard`] first.
    pub async fn unregister(self) -> zbus::Result<()> {
        let path = ObjectPath::from_str_unchecked(&self.path);
        let adapter = ObjectPath::from_str_unchecked(&self.adapter);

        let bluez_res = async {
            GattManagerProxy::builder(&self.conn)
                .path(adapter)
                .build()
                .await?
                .unregister_application(&path)
                .await
        }
        .await;

        let unexport_res = unexport(self.conn.object_server(), &path, &self.interfaces).await;
        bluez_res.and(unexport_res)
    }
}

/// Builder for an [`Application`].
pub struct ApplicationBuilder {
    path: Box<str>,
    services: Vec<ServiceBuilder>,
}

impl ApplicationBuilder {
    /// Adds a service.
    pub fn service(mut self, service: ServiceBuilder) -> Self {
        self.services.push(service);
        self
    }

    /// Exports the application and registers it with the BlueZ `adapter`
    /// (e.g. `"hci0"`).
    ///
    /// # Errors
    ///
    /// Returns an error if `path` or `adapter` is not a valid object path
    /// component, or if D-Bus export or BlueZ registration fails. Anything
    /// already exported is removed on failure.
    pub async fn register(self, conn: &Connection, adapter: &str) -> zbus::Result<Application> {
        let path = ObjectPath::try_from(&*self.path)?;
        let adapter = format!("/org/bluez/{adapter}").into_boxed_str();
        let gatt_mgr = GattManagerProxy::builder(conn)
            .path(ObjectPath::try_from(&*adapter)?)
            .build()
            .await?;

        let server = conn.object_server();
        let mut interfaces = Vec::with_capacity(self.services.len());

        let result = async {
            export(server, &path, self.services, &mut interfaces).await?;
            gatt_mgr.register_application(&path, HashMap::new()).await
        }
        .await;

        if let Err(e) = result {
            let _ = unexport(server, &path, &interfaces).await;
            return Err(e);
        }

        interfaces.sort_unstable_by_key(|i| i.key);

        Ok(Application {
            conn: conn.clone(),
            path: self.path,
            adapter,
            interfaces: interfaces.into_boxed_slice(),
        })
    }
}

async fn export(
    server: &ObjectServer,
    path: &ObjectPath<'_>,
    services: Vec<ServiceBuilder>,
    out: &mut Vec<ExportedInterface>,
) -> zbus::Result<()> {
    server.at(path, zbus::fdo::ObjectManager).await?;

    for (idx, service) in services.into_iter().enumerate() {
        let (service_uuid, service, chcrs) = service.build();
        let service_path = ObjectPath::from_string_unchecked(format!("{path}/srvc{}", idx + 1));

        server.at(&service_path, service).await?;
        out.push(ExportedInterface {
            key: (service_uuid, Uuid::nil()),
            path: service_path.clone(),
            inner: None,
        });

        out.reserve(chcrs.len());
        for (idx, chcr) in chcrs.into_iter().enumerate() {
            let chcr_uuid = chcr.uuid();
            let chcr_path =
                ObjectPath::from_string_unchecked(format!("{service_path}/chcr{}", idx + 1));

            let inner = chcr
                .build(server, service_path.clone(), chcr_path.clone())
                .await?;

            out.push(ExportedInterface {
                key: (service_uuid, chcr_uuid),
                path: chcr_path,
                inner: Some(inner),
            });
        }
    }

    Ok(())
}

async fn unexport(
    server: &ObjectServer,
    path: &ObjectPath<'_>,
    interfaces: &[ExportedInterface],
) -> zbus::Result<()> {
    const CHCR_IFACE: InterfaceName =
        InterfaceName::from_static_str_unchecked("org.bluez.GattCharacteristic1");

    let mut first_err = None;

    for iface in interfaces.iter().rev() {
        let res = if iface.inner.is_none() {
            server.remove::<Service, _>(&iface.path).await
        } else {
            server.remove_named(&iface.path, CHCR_IFACE).await
        };

        if let Err(e) = res {
            first_err.get_or_insert(e);
        }
    }

    if let Err(e) = server.remove::<zbus::fdo::ObjectManager, _>(path).await {
        first_err.get_or_insert(e);
    }

    first_err.map_or(Ok(()), Err)
}

#[zbus::proxy(interface = "org.bluez.GattManager1", default_service = "org.bluez")]
trait GattManager {
    fn register_application(
        &self,
        application: &ObjectPath<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;

    fn unregister_application(&self, application: &ObjectPath<'_>) -> zbus::Result<()>;
}
