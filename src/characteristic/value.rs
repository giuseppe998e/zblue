use super::CharacteristicError;

/// Why a remote write was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    /// The write offset is not valid for the value.
    InvalidOffset,

    /// The written length is not valid for the value.
    InvalidLength,
}

impl From<WriteError> for CharacteristicError {
    fn from(err: WriteError) -> Self {
        match err {
            WriteError::InvalidOffset => Self::InvalidOffset,
            WriteError::InvalidLength => Self::InvalidValueLength,
        }
    }
}

/// A type usable as a characteristic value.
///
/// Implemented for:
/// - `Vec<u8>` and `Vec<i8>` (variable length, one byte per element);
/// - `Vec<T>` for `T` in `u16`, `i16`, `u32`, `i32`, `u64`, `i64`
///   (variable length, a whole number of elements);
/// - the fixed-width integers `u8` through `i64`.
///
/// Multi-byte integers are stored little-endian, as mandated by GATT.
///
/// Implementations must keep [`Self::Repr`] decodable: [`Self::from_repr`]
/// must not panic for any `Repr` produced by [`Self::write_repr`] or by a
/// successful [`Self::store`].
pub trait Value: 'static {
    /// Byte storage backing the value.
    type Repr: AsRef<[u8]> + Default + Clone + Send + Sync;

    /// Decodes a stored representation into the value.
    fn from_repr(repr: &Self::Repr) -> Self;

    /// Encodes the value into `repr`, replacing its previous contents.
    fn write_repr(&self, repr: &mut Self::Repr);

    /// Applies a remote write of `bytes` at `offset` to `curr`.
    ///
    /// Fixed-width types only accept a write of exactly their size at offset
    /// 0. Variable-length types replace everything from `offset` onwards with
    /// `bytes`, and require `offset` and `bytes.len()` to be whole elements.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError`] if the offset or length is invalid for the type.
    /// `curr` is left unchanged on error.
    fn store(curr: &mut Self::Repr, offset: usize, bytes: &[u8]) -> Result<(), WriteError>;
}

impl Value for Vec<u8> {
    type Repr = Self;

    fn from_repr(repr: &Self::Repr) -> Self {
        repr.clone()
    }

    fn write_repr(&self, repr: &mut Self::Repr) {
        repr.clear();
        repr.extend_from_slice(self);
    }

    fn store(curr: &mut Self::Repr, offset: usize, bytes: &[u8]) -> Result<(), WriteError> {
        if offset > curr.len() {
            return Err(WriteError::InvalidOffset);
        }

        curr.truncate(offset);
        curr.extend_from_slice(bytes);
        Ok(())
    }
}

impl Value for Vec<i8> {
    type Repr = Vec<u8>;

    fn from_repr(repr: &Self::Repr) -> Self {
        let mut v = core::mem::ManuallyDrop::new(repr.clone());

        // SAFETY: `u8` and `i8` have identical layout (size 1, align 1), all bit
        // patterns are valid, and `ManuallyDrop` prevents a double-free.
        unsafe { Vec::from_raw_parts(v.as_mut_ptr().cast::<i8>(), v.len(), v.capacity()) }
    }

    fn write_repr(&self, repr: &mut Self::Repr) {
        // SAFETY: `u8` and `i8` have identical layout (size 1, align 1), all bit
        // patterns are valid, and the slice borrows `self` for its whole lifetime.
        let bytes = unsafe { core::slice::from_raw_parts(self.as_ptr().cast::<u8>(), self.len()) };

        repr.clear();
        repr.extend_from_slice(bytes);
    }

    fn store(curr: &mut Self::Repr, offset: usize, bytes: &[u8]) -> Result<(), WriteError> {
        if offset > curr.len() {
            return Err(WriteError::InvalidOffset);
        }

        curr.truncate(offset);
        curr.extend_from_slice(bytes);
        Ok(())
    }
}

macro_rules! number_value {
    ( $( $ty:ty ),* ) => {
        $(
            impl Value for $ty {
                type Repr = [u8; { ::core::mem::size_of::<$ty>() }];

                #[inline]
                fn from_repr(repr: &Self::Repr) -> Self {
                    <$ty>::from_le_bytes(*repr)
                }

                #[inline]
                fn write_repr(&self, repr: &mut Self::Repr) {
                    *repr = self.to_le_bytes();
                }

                fn store(
                    curr: &mut Self::Repr,
                    offset: usize,
                    bytes: &[u8],
                ) -> Result<(), WriteError> {
                    if offset != 0 {
                        return Err(WriteError::InvalidOffset);
                    }

                    *curr = bytes.try_into().map_err(|_| WriteError::InvalidLength)?;
                    Ok(())
                }
            }
        )*
    };
}

macro_rules! number_vec {
    ( $( $ty:ty ),* ) => {
        $(
            impl Value for Vec<$ty> {
                type Repr = Vec<u8>;

                /// Decodes a stored representation into the value.
                ///
                /// # Panics
                ///
                /// Panics if `repr` is not a whole number of elements. This cannot
                /// happen for a `repr` produced by [`Self::write_repr`] or by a
                /// successful [`Self::store`].
                fn from_repr(repr: &Self::Repr) -> Self {
                    let (chunks, []) = repr.as_chunks::<{ ::core::mem::size_of::<$ty>() }>() else {
                        panic!("partial element in stored value");
                    };

                    chunks.iter().map(|b| <$ty>::from_le_bytes(*b)).collect()
                }

                fn write_repr(&self, repr: &mut Self::Repr) {
                    let byte_size = ::core::mem::size_of_val(self.as_slice());
                    repr.clear();

                    // On little-endian targets the in-memory layout already is the
                    // wire format, so the slice can be copied as is.
                    #[cfg(target_endian = "little")]
                    {
                        // SAFETY: `$ty` is a padding-free integer, every byte of it is
                        // initialized, `u8` has alignment 1, and the slice borrows
                        // `self` for its whole lifetime.
                        let bytes = unsafe {
                            ::core::slice::from_raw_parts(self.as_ptr().cast::<u8>(), byte_size)
                        };

                        repr.extend_from_slice(bytes);
                    }

                    #[cfg(not(target_endian = "little"))]
                    {
                        repr.reserve(byte_size);
                        repr.extend(self.iter().flat_map(|n| n.to_le_bytes()));
                    }
                }

                fn store(
                    curr: &mut Self::Repr,
                    offset: usize,
                    bytes: &[u8],
                ) -> Result<(), WriteError> {
                    const SIZE: usize = ::core::mem::size_of::<$ty>();

                    if bytes.len() % SIZE != 0 {
                        return Err(WriteError::InvalidLength);
                    }
                    if offset > curr.len() || offset % SIZE != 0 {
                        return Err(WriteError::InvalidOffset);
                    }

                    curr.truncate(offset);
                    curr.extend_from_slice(bytes);
                    Ok(())
                }
            }
        )*
    };
}

number_value!(u8, i8, u16, i16, u32, i32, u64, i64);
number_vec!(u16, i16, u32, i32, u64, i64);
