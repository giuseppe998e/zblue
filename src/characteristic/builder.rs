use std::{any::Any, pin::Pin, sync::Arc};

use uuid::Uuid;
use zbus::{ObjectPath, ObjectServer};

use super::{Characteristic, CharacteristicInner, Flag, Value};

type BuildFuture<'a> =
    Pin<Box<dyn Future<Output = zbus::Result<Arc<dyn Any + Send + Sync>>> + Send + 'a>>;

/// Type-erased [`CharacteristicBuilder`], so characteristics of different
/// value types can be stored together in a service.
pub(crate) trait ErasedCharacteristicBuilder {
    fn uuid(&self) -> Uuid;

    /// Exports the characteristic at `path` and returns its shared state.
    fn build<'a>(
        self: Box<Self>,
        server: &'a ObjectServer,
        service: ObjectPath<'static>,
        path: ObjectPath<'static>,
    ) -> BuildFuture<'a>;
}

/// Builder for a [`Characteristic`], added to a service with
/// [`ServiceBuilder::characteristic`](crate::ServiceBuilder::characteristic).
pub struct CharacteristicBuilder<T: Value> {
    uuid: Uuid,
    flag: Flag,
    repr: T::Repr,
}

impl<T: Value> CharacteristicBuilder<T> {
    pub(super) fn new(uuid: Uuid) -> Self {
        Self {
            uuid,
            flag: Flag::empty(),
            repr: T::Repr::default(),
        }
    }

    /// Adds `flag` to the characteristic's flags.
    pub fn flag(mut self, flag: Flag) -> Self {
        self.flag |= flag;
        self
    }

    /// Sets the initial value. Defaults to `T::Repr::default()`.
    pub fn value(mut self, value: T) -> Self {
        value.write_repr(&mut self.repr);
        self
    }
}

impl<T: Value> ErasedCharacteristicBuilder for CharacteristicBuilder<T> {
    fn uuid(&self) -> Uuid {
        self.uuid
    }

    fn build<'a>(
        self: Box<Self>,
        server: &'a ObjectServer,
        service: ObjectPath<'static>,
        path: ObjectPath<'static>,
    ) -> BuildFuture<'a> {
        let inner = Arc::new(CharacteristicInner::<T>::new(
            self.uuid, self.flag, service, self.repr,
        ));
        let chcr = Characteristic {
            inner: Arc::clone(&inner),
        };

        Box::pin(async move {
            server.at(path, chcr).await?;
            Ok(inner as Arc<dyn Any + Send + Sync>)
        })
    }
}
