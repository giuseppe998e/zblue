use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use uuid::Uuid;
use zbus::{ObjectPath, Variant};

mod types;

use self::types::{ManufacturerData, ServiceUuids};
use super::Application;

/// Configuration of an LE advertisement.
#[derive(Debug, Default)]
pub struct AdvertisementConfig<'a> {
    /// Name included in the advertisement.
    pub local_name: String,
    /// Manufacturer data as `(company identifier, payload)` pairs.
    ///
    /// Payloads longer than 24 bytes are truncated.
    pub manufacturer_data: &'a [(u16, &'a [u8])],
    /// UUIDs of the services to advertise.
    pub service_uuids: &'a [Uuid],
}

/// A registered advertisement, returned by
/// [`Application::advertise`](crate::Application::advertise).
///
/// Dropping the guard does not stop the advertisement; call
/// [`Self::unregister`].
pub struct AdvertisementGuard<'a> {
    app: &'a Application,
    released: Arc<AtomicBool>,
}

impl AdvertisementGuard<'_> {
    /// Stops the advertisement and removes it from D-Bus.
    ///
    /// Cleanup always runs to completion; the first error is returned.
    pub async fn unregister(self) -> zbus::Result<()> {
        let Application {
            conn,
            path,
            adapter,
            ..
        } = self.app;

        let path = format!("{path}/adv");
        let path = ObjectPath::from_str_unchecked(&path);

        let bluez = if self.released.load(Ordering::Acquire) {
            Ok(())
        } else {
            async {
                AdvertisingManagerProxy::builder(conn)
                    .path(ObjectPath::from_str_unchecked(adapter))
                    .build()
                    .await?
                    .unregister_advertisement(&path)
                    .await
            }
            .await
        };

        let local = conn.object_server().remove::<Advertisement, _>(&path).await;
        bluez.and(local.map(drop))
    }
}

/// The `org.bluez.LEAdvertisement1` object exported on D-Bus.
pub(super) struct Advertisement {
    local_name: String,
    manufacturer_data: ManufacturerData,
    service_uuids: ServiceUuids,
    released: Arc<AtomicBool>,
}

impl Advertisement {
    pub(super) async fn register<'a>(
        app: &'a Application,
        conf: AdvertisementConfig<'_>,
    ) -> zbus::Result<AdvertisementGuard<'a>> {
        let path = format!("{}/adv", app.path);
        let path = ObjectPath::from_str_unchecked(&path);

        let released = Arc::new(AtomicBool::new(false));
        let advertisement = Advertisement {
            local_name: conf.local_name,
            manufacturer_data: ManufacturerData::new(conf.manufacturer_data),
            service_uuids: ServiceUuids::new(conf.service_uuids),
            released: Arc::clone(&released),
        };

        let server = app.conn.object_server();
        server.at(&path, advertisement).await?;

        let result = async {
            AdvertisingManagerProxy::builder(&app.conn)
                .path(ObjectPath::from_str_unchecked(&app.adapter))
                .build()
                .await?
                .register_advertisement(&path, HashMap::new())
                .await
        }
        .await;

        match result {
            Ok(()) => Ok(AdvertisementGuard { app, released }),
            Err(e) => {
                let _ = server.remove::<Self, _>(&path).await;
                Err(e)
            }
        }
    }
}

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    #[zbus(property(emits_changed_signal = "const"), name = "Type")]
    fn kind(&self) -> &str {
        "peripheral"
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn local_name(&self) -> &str {
        &self.local_name
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn manufacturer_data(&self) -> &ManufacturerData {
        &self.manufacturer_data
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn service_uuids(&self) -> &ServiceUuids {
        &self.service_uuids
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
    }
}

#[zbus::proxy(
    interface = "org.bluez.LEAdvertisingManager1",
    default_service = "org.bluez"
)]
trait AdvertisingManager {
    fn register_advertisement(
        &self,
        advertisement: &ObjectPath<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;

    fn unregister_advertisement(&self, advertisement: &ObjectPath<'_>) -> zbus::Result<()>;
}
