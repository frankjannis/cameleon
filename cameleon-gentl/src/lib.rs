pub mod device;
pub mod interface;
pub mod port;
pub mod system;

use thiserror::Error;

use cameleon::device::DeviceError;
use cameleon_impl::memory::MemoryError;

#[derive(Error, Debug)]
pub enum GenTlError {
    /// The handle isn't opened.
    #[error("the handle isn't opened")]
    NotOpened,

    /// The access to the requested register address is denied because the register is not writable
    /// or because the Port module is opened in a way that it does not allow write access.
    #[error("the access to the requested register addresss is denied")]
    AccessDenied,

    /// There is no register with the provided address.
    #[error("there is no register with the provided address")]
    InvalidAddress,

    /// An invalid value has been written.
    #[error("an invalid value has been written: {}", 0)]
    InvalidValue(std::borrow::Cow<'static, str>),

    /// Communication error or connection lost.
    #[error("communication error or connection lost: {}", 0)]
    IoError(Box<dyn std::error::Error>),

    /// Requested resource is already in use.
    #[error("requested resource is already in use")]
    ResourceInUse,

    /// ID doesn't reference any module or remote device.
    #[error("given ID doesn't reference any module or remote device: {}", 0)]
    InvalidId(String),

    /// A provided index referencing a Producer internal object is out of bounds.
    #[error("given index is out of range")]
    InvalidIndex,

    /// Operation timed out.
    #[error("operation timed out")]
    Timeout,
}

impl From<MemoryError> for GenTlError {
    fn from(err: MemoryError) -> Self {
        match err {
            MemoryError::AddressNotReadable | MemoryError::AddressNotWritable => Self::AccessDenied,
            MemoryError::InvalidAddress => Self::InvalidAddress,
            MemoryError::InvalidRegisterData(cause) => Self::InvalidValue(cause),
        }
    }
}

impl From<DeviceError> for GenTlError {
    fn from(err: DeviceError) -> Self {
        use GenTlError::*;

        match err {
            DeviceError::Busy => ResourceInUse,
            DeviceError::Disconnected | DeviceError::Io(..) | DeviceError::InternalError(..) => {
                IoError(err.into())
            }
            DeviceError::NotOpened => NotOpened,
            DeviceError::InvalidData(..) => InvalidValue(format!("{}", err).into()),
            DeviceError::Timeout => Timeout,
        }
    }
}

pub type GenTlResult<T> = std::result::Result<T, GenTlError>;
