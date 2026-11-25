use uuid::Uuid;

use super::characteristic::{CharacteristicBuilder, ErasedCharacteristicBuilder, Value};

/// A GATT service exported as `org.bluez.GattService1`.
///
/// Created with [`Service::builder`] and added to an
/// [`Application`](crate::Application).
pub struct Service {
    uuid: Uuid,
    primary: bool,
}

impl Service {
    /// Returns a builder for a service identified by `uuid`.
    ///
    /// The service is secondary unless [`ServiceBuilder::primary`] is set.
    pub fn builder(uuid: Uuid) -> ServiceBuilder {
        ServiceBuilder {
            inner: Self {
                uuid,
                primary: false,
            },
            chcrs: Vec::new(),
        }
    }
}

#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(name = "UUID", property(emits_changed_signal = "const"))]
    pub(super) fn uuid(&self) -> String {
        self.uuid.to_string()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn primary(&self) -> bool {
        self.primary
    }
}

/// Builder for a [`Service`].
pub struct ServiceBuilder {
    inner: Service,
    chcrs: Vec<Box<dyn ErasedCharacteristicBuilder>>,
}

impl ServiceBuilder {
    /// Sets whether the service is primary.
    pub fn primary(mut self, primary: bool) -> Self {
        self.inner.primary = primary;
        self
    }

    /// Adds a characteristic.
    pub fn characteristic<T: Value>(mut self, chcr: CharacteristicBuilder<T>) -> Self {
        self.chcrs.push(Box::new(chcr));
        self
    }

    /// Splits the builder into the service UUID, the service and its characteristics.
    pub(super) fn build(self) -> (Uuid, Service, Vec<Box<dyn ErasedCharacteristicBuilder>>) {
        (self.inner.uuid, self.inner, self.chcrs)
    }
}
