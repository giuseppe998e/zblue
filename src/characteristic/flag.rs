use std::ops::{BitOr, BitOrAssign};

/// A set of BlueZ GATT characteristic flags.
///
/// Bit positions are internal and do not match the GATT properties bitfield.
///
/// Security and MIC flags (e.g. [`SECURE_READ`]) also set their base bit
/// (`READ`, `WRITE`, `NOTIFY` or `INDICATE`).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flag(u32);

// Characteristic properties

/// Characteristic supports broadcasting.
pub const BROADCAST: Flag = Flag(1 << 0);

/// Characteristic supports read operations.
pub const READ: Flag = Flag(1 << 1);

/// Characteristic supports write operations without response.
pub const WRITE_WITHOUT_RESPONSE: Flag = Flag(1 << 2);

/// Characteristic supports write operations with response.
pub const WRITE: Flag = Flag(1 << 3);

/// Characteristic supports notifications.
pub const NOTIFY: Flag = Flag(1 << 4);

/// Characteristic supports indications.
pub const INDICATE: Flag = Flag(1 << 5);

/// Characteristic supports authenticated signed writes.
pub const AUTHENTICATED_SIGNED_WRITES: Flag = Flag(1 << 6);

/// Characteristic has extended properties.
pub const EXTENDED_PROPERTIES: Flag = Flag(1 << 22);

/// Characteristic supports reliable writes.
pub const RELIABLE_WRITE: Flag = Flag(1 << 7);

/// Characteristic supports writable auxiliary descriptors.
pub const WRITABLE_AUXILIARIES: Flag = Flag(1 << 8);

// Security requirements (composite with READ / WRITE)

/// Reading the characteristic requires an encrypted connection (implies `READ`).
pub const ENCRYPT_READ: Flag = Flag(READ.0 | (1 << 9));

/// Writing the characteristic requires an encrypted connection (implies `WRITE`).
pub const ENCRYPT_WRITE: Flag = Flag(WRITE.0 | (1 << 10));

/// Reading the characteristic requires an authenticated encrypted connection (implies `READ`).
pub const ENCRYPT_AUTHENTICATED_READ: Flag = Flag(READ.0 | (1 << 11));

/// Writing the characteristic requires an authenticated encrypted connection (implies `WRITE`).
pub const ENCRYPT_AUTHENTICATED_WRITE: Flag = Flag(WRITE.0 | (1 << 12));

/// Reading the characteristic requires a secure connection (implies `READ`).
pub const SECURE_READ: Flag = Flag(READ.0 | (1 << 13));

/// Writing the characteristic requires a secure connection (implies `WRITE`).
pub const SECURE_WRITE: Flag = Flag(WRITE.0 | (1 << 14));

/// Access to the characteristic requires authorization.
pub const AUTHORIZE: Flag = Flag(1 << 15);

// Notification and indication security requirements (composite with NOTIFY / INDICATE).
// These flags are server-only in BlueZ.

/// Notifications require an encrypted connection (implies `NOTIFY`).
pub const ENCRYPT_NOTIFY: Flag = Flag(NOTIFY.0 | (1 << 16));

/// Indications require an encrypted connection (implies `INDICATE`).
pub const ENCRYPT_INDICATE: Flag = Flag(INDICATE.0 | (1 << 17));

/// Notifications require an authenticated encrypted connection (implies `NOTIFY`).
pub const ENCRYPT_AUTHENTICATED_NOTIFY: Flag = Flag(NOTIFY.0 | (1 << 18));

/// Indications require an authenticated encrypted connection (implies `INDICATE`).
pub const ENCRYPT_AUTHENTICATED_INDICATE: Flag = Flag(INDICATE.0 | (1 << 19));

/// Notifications require a secure connection (implies `NOTIFY`).
pub const SECURE_NOTIFY: Flag = Flag(NOTIFY.0 | (1 << 20));

/// Indications require a secure connection (implies `INDICATE`).
pub const SECURE_INDICATE: Flag = Flag(INDICATE.0 | (1 << 21));

// Message Integrity Check (MIC) length requirements (composite with READ / WRITE).
// These flags are server-only in BlueZ.

/// Reading requires a connection with a MIC length of at least 64 bits (implies `READ`).
pub const MIC_64_READ: Flag = Flag(READ.0 | (1 << 23));

/// Writing requires a connection with a MIC length of at least 64 bits (implies `WRITE`).
pub const MIC_64_WRITE: Flag = Flag(WRITE.0 | (1 << 24));

/// Reading requires a connection with a MIC length of at least 128 bits (implies `READ`).
pub const MIC_128_READ: Flag = Flag(READ.0 | (1 << 25));

/// Writing requires a connection with a MIC length of at least 128 bits (implies `WRITE`).
pub const MIC_128_WRITE: Flag = Flag(WRITE.0 | (1 << 26));

// Masks for the non-base bits of each specialized flag family.
const SPECIALIZED_READ_MASK: u32 = (ENCRYPT_READ.0
    | ENCRYPT_AUTHENTICATED_READ.0
    | SECURE_READ.0
    | MIC_64_READ.0
    | MIC_128_READ.0)
    & !READ.0;

const SPECIALIZED_WRITE_MASK: u32 = (ENCRYPT_WRITE.0
    | ENCRYPT_AUTHENTICATED_WRITE.0
    | SECURE_WRITE.0
    | MIC_64_WRITE.0
    | MIC_128_WRITE.0)
    & !WRITE.0;

const SPECIALIZED_NOTIFY_MASK: u32 =
    (ENCRYPT_NOTIFY.0 | ENCRYPT_AUTHENTICATED_NOTIFY.0 | SECURE_NOTIFY.0) & !NOTIFY.0;

const SPECIALIZED_INDICATE_MASK: u32 =
    (ENCRYPT_INDICATE.0 | ENCRYPT_AUTHENTICATED_INDICATE.0 | SECURE_INDICATE.0) & !INDICATE.0;

/// Maps internal flags to their corresponding BlueZ D-Bus strings.
static TABLE: &[(Flag, &str)] = &[
    (BROADCAST, "broadcast"),
    (READ, "read"),
    (WRITE_WITHOUT_RESPONSE, "write-without-response"),
    (WRITE, "write"),
    (NOTIFY, "notify"),
    (INDICATE, "indicate"),
    (AUTHENTICATED_SIGNED_WRITES, "authenticated-signed-writes"),
    (EXTENDED_PROPERTIES, "extended-properties"),
    (RELIABLE_WRITE, "reliable-write"),
    (WRITABLE_AUXILIARIES, "writable-auxiliaries"),
    (ENCRYPT_READ, "encrypt-read"),
    (ENCRYPT_WRITE, "encrypt-write"),
    (ENCRYPT_NOTIFY, "encrypt-notify"),
    (ENCRYPT_INDICATE, "encrypt-indicate"),
    (ENCRYPT_AUTHENTICATED_READ, "encrypt-authenticated-read"),
    (ENCRYPT_AUTHENTICATED_WRITE, "encrypt-authenticated-write"),
    (ENCRYPT_AUTHENTICATED_NOTIFY, "encrypt-authenticated-notify"),
    (
        ENCRYPT_AUTHENTICATED_INDICATE,
        "encrypt-authenticated-indicate",
    ),
    (SECURE_READ, "secure-read"),
    (SECURE_WRITE, "secure-write"),
    (SECURE_NOTIFY, "secure-notify"),
    (SECURE_INDICATE, "secure-indicate"),
    (MIC_64_READ, "mic-64-read"),
    (MIC_64_WRITE, "mic-64-write"),
    (MIC_128_READ, "mic-128-read"),
    (MIC_128_WRITE, "mic-128-write"),
    (AUTHORIZE, "authorize"),
];

impl Flag {
    /// Returns an empty flag set.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns the underlying bit representation.
    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns `true` if all bits of `other` are set.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Returns `true` if any bit of `other` is set.
    ///
    /// Composite flags of the same family share a base bit, so they intersect
    /// each other. Prefer [`Self::contains`] to test for a specific flag.
    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Returns the union of two flag sets.
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns `true` if no flags are set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Parses a single BlueZ flag string, or returns `None` if unrecognized.
    pub fn from_bluez_str(flag: &str) -> Option<Self> {
        TABLE
            .iter()
            .find_map(|&(value, name)| (name == flag).then_some(value))
    }

    /// Parses BlueZ flag strings into a flag set.
    ///
    /// # Errors
    ///
    /// Returns the first unrecognized string.
    pub fn from_bluez_flags<'a, I>(flags: I) -> Result<Self, &'a str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut result = Self::empty();

        for name in flags {
            let flag = Self::from_bluez_str(name).ok_or(name)?;
            result |= flag;
        }

        Ok(result)
    }

    /// Returns the BlueZ strings of the set flags, in table order.
    ///
    /// `"read"`, `"write"`, `"notify"` and `"indicate"` are omitted when a
    /// specialized flag of the same family is set.
    pub fn iter_bluez_strings(self) -> impl Iterator<Item = &'static str> {
        let suppress_read = (self.0 & SPECIALIZED_READ_MASK) != 0;
        let suppress_write = (self.0 & SPECIALIZED_WRITE_MASK) != 0;
        let suppress_notify = (self.0 & SPECIALIZED_NOTIFY_MASK) != 0;
        let suppress_indicate = (self.0 & SPECIALIZED_INDICATE_MASK) != 0;

        TABLE.iter().filter_map(move |&(flag, name)| {
            let suppressed = (flag == READ && suppress_read)
                || (flag == WRITE && suppress_write)
                || (flag == NOTIFY && suppress_notify)
                || (flag == INDICATE && suppress_indicate);

            (!suppressed && self.contains(flag)).then_some(name)
        })
    }
}

impl BitOr for Flag {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for Flag {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
