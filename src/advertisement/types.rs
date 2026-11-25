use serde_core::{
    Serialize,
    ser::{self, SerializeMap, SerializeSeq as _},
};
use uuid::Uuid;
use zbus::{Signature, Type};

/// Longest manufacturer payload that fits in a legacy 31-byte advertisement
/// alongside the flags (3 bytes) and the manufacturer AD header (4 bytes).
const MAX_MANUFACTURER_DATA: usize = 24;

/// Manufacturer data keyed by company identifier, serialized as `a{qv}`.
///
/// Payloads are truncated to [`MAX_MANUFACTURER_DATA`] bytes and stored
/// inline to avoid per-entry allocations.
pub(super) struct ManufacturerData(Box<[(u16, usize, [u8; MAX_MANUFACTURER_DATA])]>);

impl ManufacturerData {
    pub(super) fn new(entries: &[(u16, &[u8])]) -> Self {
        let entries = entries
            .iter()
            .map(|&(company, data)| {
                let len = data.len().min(MAX_MANUFACTURER_DATA);

                let mut buf = [0u8; MAX_MANUFACTURER_DATA];
                buf[..len].copy_from_slice(&data[..len]);

                (company, len, buf)
            })
            .collect();

        Self(entries)
    }
}

impl Serialize for ManufacturerData {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;

        for (company, len, data) in &self.0 {
            let data = &data[..*len];
            map.serialize_entry(company, &zbus::zvariant::SerializeValue(&data))?;
        }

        map.end()
    }
}

impl Type for ManufacturerData {
    const SIGNATURE: &'static Signature =
        &Signature::static_dict(&Signature::U16, &Signature::Variant);
}

/// Service UUIDs, serialized as `as` of lowercase hyphenated strings.
pub(super) struct ServiceUuids(Box<[Uuid]>);

impl ServiceUuids {
    pub(super) fn new(uuids: &[Uuid]) -> Self {
        Self(uuids.into())
    }
}

impl Serialize for ServiceUuids {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use uuid::fmt::Hyphenated;

        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;

        for uuid in &self.0 {
            let mut buf = [0u8; Hyphenated::LENGTH];
            let uuid: &str = uuid.as_hyphenated().encode_lower(&mut buf);
            seq.serialize_element(uuid)?;
        }

        seq.end()
    }
}

impl Type for ServiceUuids {
    const SIGNATURE: &'static Signature = &Signature::static_array(&Signature::Str);
}
