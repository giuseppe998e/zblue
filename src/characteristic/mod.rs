use std::borrow::Borrow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Notify;
use uuid::Uuid;
use zbus::object_server::{Interface, SignalEmitter};
use zbus::{DBusError, ObjectPath};

mod builder;
pub mod flag;
mod types;
mod value;

pub use self::builder::CharacteristicBuilder;
pub(super) use self::builder::ErasedCharacteristicBuilder;
pub use self::flag::Flag;
use self::types::{ByteBuf, Bytes, Options, PropertyEntry, SubSlice};
pub use self::value::{Value, WriteError};

/// Errors returned to `bluetoothd` from characteristic methods.
#[derive(Debug, DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum CharacteristicError {
    InvalidOffset,
    InvalidValueLength,
    NotSupported,
    #[zbus(error)]
    ZBus(zbus::Error),
}

/// A GATT characteristic exported as `org.bluez.GattCharacteristic1`.
///
/// Created with [`Characteristic::builder`] and added to a [`Service`](crate::Service).
/// Remote reads and writes are served from the stored value; use a
/// [`CharacteristicHandle`] to access it locally.
pub struct Characteristic<T: Value> {
    inner: Arc<CharacteristicInner<T>>,
}

impl<T: Value> Characteristic<T> {
    /// Returns a builder for a characteristic identified by `uuid`.
    pub fn builder(uuid: Uuid) -> CharacteristicBuilder<T> {
        CharacteristicBuilder::new(uuid)
    }
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl<T: Value> Characteristic<T> {
    #[zbus(name = "UUID", property(emits_changed_signal = "const"))]
    fn uuid(&self) -> String {
        self.inner.uuid.to_string()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn flags(&self) -> Vec<&'static str> {
        self.inner.flag.iter_bluez_strings().collect()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn service(&self) -> &ObjectPath<'static> {
        &self.inner.service
    }

    #[zbus(property)]
    fn notifying(&self) -> bool {
        self.inner.notifying.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn value(&self) -> ByteBuf<T::Repr> {
        let repr = self.inner.state().repr.clone();
        ByteBuf(repr)
    }

    fn read_value(
        &self,
        options: Options,
    ) -> Result<ByteBuf<SubSlice<T::Repr>>, CharacteristicError> {
        let state = self.inner.state();

        SubSlice::new(&state.repr, options.offset)
            .ok_or(CharacteristicError::InvalidOffset)
            .map(ByteBuf)
    }

    async fn write_value(
        &self,
        value: Bytes<'_>,
        options: Options,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), CharacteristicError> {
        let (_, notify) = self
            .inner
            .update(|repr| T::store(repr, options.offset, value.0))?;

        if let Some(repr) = notify {
            value_changed::<T>(&emitter, repr).await?;
        }

        Ok(())
    }

    async fn start_notify(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), CharacteristicError> {
        const NOTIFY_SUPPORTED: Flag = flag::NOTIFY.union(flag::INDICATE);

        if !self.inner.flag.intersects(NOTIFY_SUPPORTED) {
            return Err(CharacteristicError::NotSupported);
        }

        if !self.inner.notifying.swap(true, Ordering::Relaxed) {
            self.notifying_changed(&emitter).await?;
        }

        Ok(())
    }

    async fn stop_notify(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), CharacteristicError> {
        if self.inner.notifying.swap(false, Ordering::Relaxed) {
            self.notifying_changed(&emitter).await?;
        }

        Ok(())
    }
}

/// A handle for reading, modifying and observing a characteristic.
///
/// Obtained from [`Application::get_characteristic`](crate::Application::get_characteristic).
/// Cloned handles observe changes independently.
#[derive(Clone)]
pub struct CharacteristicHandle<T: Value> {
    inner: Arc<CharacteristicInner<T>>,
    emitter: SignalEmitter<'static>,
    last_seen: u64,
}

impl<T: Value> CharacteristicHandle<T> {
    pub(super) fn new(inner: Arc<CharacteristicInner<T>>, emitter: SignalEmitter<'static>) -> Self {
        let last_seen = inner.state().version;

        Self {
            inner,
            emitter,
            last_seen,
        }
    }

    /// Returns the current value.
    pub fn get(&self) -> T {
        let repr = &self.inner.state().repr;
        T::from_repr(repr)
    }

    /// Replaces the value.
    ///
    /// Subscribed clients are notified if they started notifications. The change is not
    /// reported by [`Self::changed`] on this handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification cannot be emitted. The value is
    /// updated regardless.
    pub async fn set<V: Borrow<T>>(&mut self, value: V) -> zbus::Result<()> {
        let Ok((version, notify)) = self.inner.update(|repr| {
            value.borrow().write_repr(repr);
            Ok::<_, core::convert::Infallible>(())
        });

        self.last_seen = version;

        if let Some(repr) = notify {
            value_changed::<T>(&self.emitter, repr).await?;
        }

        Ok(())
    }

    /// Waits until the value changes and returns the latest one.
    ///
    /// Changes made before this future is polled may be coalesced into a
    /// single result. Cancel-safe.
    pub async fn changed(&mut self) -> T {
        loop {
            let notified = self.inner.state_changed.notified();

            {
                let state = self.inner.state();
                if state.version != self.last_seen {
                    self.last_seen = state.version;
                    return T::from_repr(&state.repr);
                }
            }

            notified.await;
        }
    }

    /// Returns whether the value changed since the last
    /// [`Self::changed`] or [`Self::set`] on this handle.
    pub fn has_changed(&self) -> bool {
        self.inner.state().version != self.last_seen
    }
}

/// Shared state backing a characteristic and all its handles.
pub(super) struct CharacteristicInner<T: Value> {
    uuid: Uuid,
    flag: Flag,
    service: ObjectPath<'static>,
    notifying: AtomicBool,

    state: Mutex<State<T::Repr>>,
    state_changed: Notify,
}

struct State<R> {
    repr: R,
    version: u64,
}

impl<T: Value> CharacteristicInner<T> {
    fn new(uuid: Uuid, flag: Flag, service: ObjectPath<'static>, repr: T::Repr) -> Self {
        Self {
            uuid,
            flag,
            service,
            notifying: AtomicBool::new(false),
            state: Mutex::new(State { repr, version: 0 }),
            state_changed: Notify::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State<T::Repr>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn update<F, E>(&self, f: F) -> Result<(u64, Option<T::Repr>), E>
    where
        F: FnOnce(&mut T::Repr) -> Result<(), E>,
    {
        let mut state = self.state();
        f(&mut state.repr)?;

        state.version = state.version.wrapping_add(1);
        let version = state.version;
        let snapshot = self
            .notifying
            .load(Ordering::Relaxed)
            .then(|| state.repr.clone());
        drop(state);

        self.state_changed.notify_waiters();
        Ok((version, snapshot))
    }
}

/// Emits a D-Bus `PropertiesChanged` signal for the `Value` property.
///
/// Sends the snapshot taken under the lock instead of using the generated
/// `value_changed`, which would re-read the (possibly newer) state.
async fn value_changed<T: Value>(emitter: &SignalEmitter<'_>, repr: T::Repr) -> zbus::Result<()> {
    use zbus::names::{InterfaceName, MemberName};

    const PROPERTIES_IFACE: InterfaceName<'static> =
        InterfaceName::from_static_str_unchecked("org.freedesktop.DBus.Properties");
    const PROPERTIES_CHANGED: MemberName<'static> =
        MemberName::from_static_str_unchecked("PropertiesChanged");

    emitter
        .emit(
            PROPERTIES_IFACE,
            PROPERTIES_CHANGED,
            &(
                Characteristic::<T>::name(),
                PropertyEntry("Value", &ByteBuf(repr)),
                &[] as &[&str],
            ),
        )
        .await
}
