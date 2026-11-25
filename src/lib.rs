//! Export GATT services and LE advertisements to BlueZ over D-Bus.
//!
//! Build an [`Application`] from [`Service`]s and [`Characteristic`]s,
//! register it with an adapter, then advertise it. Characteristic values are
//! read and observed at runtime through a [`CharacteristicHandle`].
//!
//! ```no_run
//! use zblue::{AdvertisementConfig, Application, Characteristic, Service};
//! use zblue::characteristic::flag;
//! use zblue::util::bluetooth_sig_uuid;
//!
//! # async fn run() -> zbus::Result<()> {
//! let conn = zbus::Connection::system().await?;
//! let service = bluetooth_sig_uuid(0x180F); // Battery Service
//! let level = bluetooth_sig_uuid(0x2A19); // Battery Level
//!
//! let app = Application::builder("/org/example/app")
//!     .service(
//!         Service::builder(service).primary(true).characteristic(
//!             Characteristic::<u8>::builder(level)
//!                 .flag(flag::READ | flag::NOTIFY)
//!                 .value(100),
//!         ),
//!     )
//!     .register(&conn, "hci0")
//!     .await?;
//!
//! let adv = app
//!     .advertise(AdvertisementConfig {
//!         local_name: "demo".into(),
//!         service_uuids: &[service],
//!         ..Default::default()
//!     })
//!     .await?;
//!
//! let mut level = app.get_characteristic::<u8>(service, level).unwrap();
//! level.set(99).await?;
//!
//! adv.unregister().await?;
//! app.unregister().await?;
//! # Ok(())
//! # }
//! ```

mod advertisement;
mod application;
pub mod characteristic;
mod service;

pub use advertisement::{AdvertisementConfig, AdvertisementGuard};
pub use application::{Application, ApplicationBuilder};
pub use characteristic::{Characteristic, CharacteristicBuilder, CharacteristicHandle};
pub use service::{Service, ServiceBuilder};

pub mod util {
    use uuid::Uuid;

    /// Expands a 16-bit Bluetooth SIG UUID to its 128-bit form.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zblue::util::bluetooth_sig_uuid;
    /// // Battery Service (0x180F)
    /// let battery_service = bluetooth_sig_uuid(0x180F);
    /// ```
    pub const fn bluetooth_sig_uuid(val: u16) -> Uuid {
        const SIG_BASE_UUID: u128 = 0x00000000_0000_1000_8000_00805F9B34FB;
        Uuid::from_u128(SIG_BASE_UUID | ((val as u128) << 96))
    }
}
