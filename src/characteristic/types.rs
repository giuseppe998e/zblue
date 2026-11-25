use std::fmt;

use serde_core::{Deserialize, Serialize, de, ser};
use zbus::{Signature, Type, Variant};

/// Method options dict (`a{sv}`); only `offset` is read.
pub(super) struct Options {
    pub offset: usize,
}

impl<'de> Deserialize<'de> for Options {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OptionsVisitor;

        impl<'de> de::Visitor<'de> for OptionsVisitor {
            type Value = Options;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a dict")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut this = Options { offset: 0 };

                while let Some(key) = map.next_key::<&'de str>()? {
                    if key == "offset" {
                        let variant = map.next_value::<Variant<'de>>()?;
                        let offset = u16::try_from(variant).map_err(de::Error::custom)?;
                        this.offset = usize::from(offset);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }

                Ok(this)
            }
        }

        deserializer.deserialize_map(OptionsVisitor)
    }
}

impl Type for Options {
    const SIGNATURE: &'static Signature =
        &Signature::static_dict(&Signature::Str, &Signature::Variant);
}

/// Single-entry `a{sv}` used as the changed-properties map of `PropertiesChanged`.
pub(super) struct PropertyEntry<'a, V>(pub &'a str, pub &'a V);

impl<V: Serialize + Type> Serialize for PropertyEntry<'_, V> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde_core::ser::SerializeMap as _;

        let mut s = serializer.serialize_map(Some(1))?;
        s.serialize_entry(self.0, &zbus::as_variant::Serialize(self.1))?;
        s.end()
    }
}

impl<V: Type> Type for PropertyEntry<'_, V> {
    const SIGNATURE: &'static Signature =
        &Signature::static_dict(&Signature::Str, &Signature::Variant);
}

/// Borrowed byte array (`ay`) argument.
pub(super) struct Bytes<'a>(pub &'a [u8]);

impl<'de> Deserialize<'de> for Bytes<'de> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <&'de [u8]>::deserialize(deserializer).map(Self)
    }
}

impl<'a> Type for Bytes<'a> {
    const SIGNATURE: &'static Signature = &Signature::static_array(&Signature::U8);
}

/// Bytes serialized as `ay`.
pub(super) struct ByteBuf<T>(pub T);

impl<T: AsRef<[u8]>> Serialize for ByteBuf<T> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = self.0.as_ref();
        serializer.serialize_bytes(bytes)
    }
}

impl<T: AsRef<[u8]>> Type for ByteBuf<T> {
    const SIGNATURE: &'static Signature = &Signature::static_array(&Signature::U8);
}

/// A value representation viewed from `offset`, for offset reads.
pub(super) struct SubSlice<R> {
    repr: R,
    offset: usize,
}

impl<R: AsRef<[u8]> + Clone> SubSlice<R> {
    pub fn new(repr: &R, offset: usize) -> Option<Self> {
        (offset <= repr.as_ref().len()).then_some(Self {
            repr: repr.clone(),
            offset,
        })
    }
}

impl<R: AsRef<[u8]>> AsRef<[u8]> for SubSlice<R> {
    fn as_ref(&self) -> &[u8] {
        let bytes = self.repr.as_ref();
        &bytes[self.offset..]
    }
}
