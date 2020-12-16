#[macro_use]
mod macros;

pub mod device;
pub mod interface;
pub mod port;
pub mod system;

use std::{cell::RefCell, mem::ManuallyDrop, sync::RwLock};

use crate::{imp, GenTlError, GenTlResult};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GC_ERROR(i32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct bool8_t(u8);

impl bool8_t {
    fn true_() -> Self {
        Self(1)
    }

    fn false_() -> Self {
        Self(0)
    }
}

impl Into<bool> for bool8_t {
    fn into(self) -> bool {
        self.0 != 0
    }
}

impl From<bool> for bool8_t {
    fn from(v: bool) -> Self {
        if v {
            Self::true_()
        } else {
            Self::false_()
        }
    }
}

impl Into<GC_ERROR> for &GenTlError {
    fn into(self) -> GC_ERROR {
        use GenTlError::*;
        let code = match self {
            Error(..) => -1001,
            NotInitialized => -1002,
            NotImplemented => -1003,
            ResourceInUse => -1004,
            AccessDenied => -1005,
            InvalidHandle => -1006,
            InvalidId(..) => -1007,
            NoData => -1008,
            InvalidParameter => -1009,
            Io(..) => -1010,
            Timeout => -1011,
            Abort => -1012,
            InvalidBuffer => -1013,
            NotAvailable => -1014,
            InvalidAddress => -1015,
            BufferTooSmall => -1016,
            InvalidIndex => -1017,
            ParsingChunkData => -1018,
            InvalidValue(..) => -1019,
            ResourceExhausted => -1020,
            OutOfMemory => -1021,
            Busy => -1022,
            Ambiguous => -1023,
        };
        GC_ERROR(code)
    }
}

impl Into<GC_ERROR> for GenTlError {
    fn into(self) -> GC_ERROR {
        (&self).into()
    }
}

impl<T> Into<GC_ERROR> for GenTlResult<T> {
    fn into(self) -> GC_ERROR {
        match self {
            Ok(..) => GC_ERROR(0),
            Err(e) => e.into(),
        }
    }
}

impl<T> Into<GC_ERROR> for &GenTlResult<T> {
    fn into(self) -> GC_ERROR {
        match self {
            Ok(..) => GC_ERROR(0),
            Err(e) => e.into(),
        }
    }
}

struct LastError {
    err: Option<GenTlError>,
}

enum ModuleHandle<'a> {
    System(system::SystemModuleRef<'a>),
    Interface(interface::InterfaceModuleRef<'a>),
    Device(device::DeviceModuleRef<'a>),
    RemoteDevice(device::RemoteDeviceRef<'a>),
}

impl<'a> ModuleHandle<'a> {
    fn system(&self) -> GenTlResult<system::SystemModuleRef<'a>> {
        match self {
            ModuleHandle::System(system) => Ok(system),
            _ => Err(GenTlError::InvalidHandle),
        }
    }

    fn interface(&self) -> GenTlResult<interface::InterfaceModuleRef<'a>> {
        match self {
            ModuleHandle::Interface(iface) => Ok(*iface),
            _ => Err(GenTlError::InvalidHandle),
        }
    }

    fn device(&self) -> GenTlResult<device::DeviceModuleRef<'a>> {
        match self {
            ModuleHandle::Device(dev) => Ok(*dev),
            _ => Err(GenTlError::InvalidHandle),
        }
    }

    unsafe fn from_raw_manually_drop(
        raw_handle: *mut libc::c_void,
    ) -> GenTlResult<ManuallyDrop<Box<ModuleHandle<'a>>>> {
        if !raw_handle.is_null() {
            let handle = raw_handle as *mut ModuleHandle;
            Ok(ManuallyDrop::new(Box::from_raw(handle)))
        } else {
            Err(GenTlError::InvalidHandle)
        }
    }

    unsafe fn into_raw(self: Box<Self>) -> *mut libc::c_void {
        Box::into_raw(self) as *mut libc::c_void
    }
}

newtype_enum! {
    pub enum INFO_DATATYPE {
        INFO_DATATYPE_UNKNOWN = 0,
        INFO_DATATYPE_STRING = 1,
        INFO_DATATYPE_STRINGLIST = 2,
        INFO_DATATYPE_INT16 = 3,
        INFO_DATATYPE_UINT16 = 4,
        INFO_DATATYPE_INT32 = 5,
        INFO_DATATYPE_UINT32 = 6,
        INFO_DATATYPE_INT64 = 7,
        INFO_DATATYPE_UINT64 = 8,
        INFO_DATATYPE_FLOAT64 = 9,
        INFO_DATATYPE_PTR = 10,
        INFO_DATATYPE_BOOL8 = 11,
        INFO_DATATYPE_SIZET = 12,
        INFO_DATATYPE_BUFFER = 13,
        INFO_DATATYPE_PTRDIFF = 14,
    }
}

lazy_static::lazy_static! {
    static ref IS_LIB_INITIALIZED: RwLock<bool> = RwLock::new(false);
}

thread_local! {
    static LAST_ERROR: RefCell<LastError> = {
        let last_error = LastError {
            err: None,
        };
        RefCell::new(last_error)
    }
}

impl crate::imp::port::TlType {
    fn as_str(self) -> &'static str {
        use super::imp::port::TlType::*;
        match self {
            CameraLink => "CL",
            CameraLinkHS => "CLHS",
            CoaXPress => "CXP",
            GigEVision => "GEV",
            USB3Vision => "U3V",
            Mixed => "Mixed",
        }
    }
}

impl crate::imp::CharEncoding {
    fn as_raw(self) -> i32 {
        match self {
            Self::Ascii => 0,
            Self::UTF8 => 1,
        }
    }
}

fn save_last_error<T>(res: GenTlResult<T>) {
    match res {
        Ok(_) => {}
        Err(e) => LAST_ERROR.with(|err| {
            let mut err = err.borrow_mut();
            err.err = Some(e);
        }),
    }
}

fn copy_info<T: CopyTo>(
    src: T,
    dst: *mut libc::c_void,
    dst_size: *mut libc::size_t,
) -> GenTlResult<INFO_DATATYPE> {
    src.copy_to(dst as *mut T::Destination, dst_size)?;
    Ok(T::info_data_type())
}

trait CopyTo {
    type Destination;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()>;

    fn info_data_type() -> INFO_DATATYPE;
}

impl CopyTo for &str {
    type Destination = libc::c_char;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        if !self.is_ascii() {
            return Err(GenTlError::InvalidValue("string is not ascii".into()));
        }

        let string_len = self.len() + 1;
        if !dst.is_null() {
            unsafe {
                if *dst_size < string_len {
                    return Err(GenTlError::BufferTooSmall);
                }
                std::ptr::copy_nonoverlapping(
                    self.as_ptr() as *const libc::c_char,
                    dst,
                    self.len(),
                );
                dst.offset(self.len() as isize).write(0); // Null terminated.
            }
        }

        unsafe {
            *dst_size = string_len;
        }

        Ok(())
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_STRING
    }
}

impl CopyTo for &[u8] {
    type Destination = u8;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        let len = self.len();
        if !dst.is_null() {
            unsafe {
                if *dst_size < len {
                    return Err(GenTlError::BufferTooSmall);
                }
                std::ptr::copy_nonoverlapping(self.as_ptr(), dst, self.len());
            }
        }

        unsafe {
            *dst_size = len;
        }

        Ok(())
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_BUFFER
    }
}

impl CopyTo for imp::port::TlType {
    type Destination = libc::c_char;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        let s = match self {
            Self::CameraLink => "CL",
            Self::CameraLinkHS => "CLHS",
            Self::CoaXPress => "CXP",
            Self::GigEVision => "GEV",
            Self::USB3Vision => "U3V",
            Self::Mixed => "Mixed",
        };

        s.copy_to(dst, dst_size)
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_STRING
    }
}

impl CopyTo for imp::port::ModuleType {
    type Destination = libc::c_char;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        let s = match self {
            Self::System => "TLSystem",
            Self::Interface => "TLInterface",
            Self::Device => "TLDevice",
            Self::DataStream => "TLDataStream",
            Self::Buffer => "TLBuffer",
            Self::RemoteDevice => "Device",
        };

        s.copy_to(dst, dst_size)
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_STRING
    }
}

impl CopyTo for bool8_t {
    type Destination = u8;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        let len = std::mem::size_of::<u8>();

        if !dst.is_null() {
            unsafe {
                if *dst_size < len {
                    return Err(GenTlError::BufferTooSmall);
                }
                *dst = self.0;
            }
        }

        unsafe {
            *dst_size = len;
        }
        Ok(())
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_BOOL8
    }
}

impl CopyTo for imp::device::DeviceAccessStatus {
    type Destination = i32;

    fn copy_to(&self, dst: *mut Self::Destination, dst_size: *mut libc::size_t) -> GenTlResult<()> {
        let val = match self {
            Self::Unknown => 0,
            Self::ReadWrite => 1,
            Self::ReadOnly => 2,
            Self::NoAccess => 3,
            Self::Busy => 4,
            Self::OpenReadWrite => 5,
            Self::OpenReadOnly => 6,
        };

        val.copy_to(dst, dst_size)
    }

    fn info_data_type() -> INFO_DATATYPE {
        INFO_DATATYPE::INFO_DATATYPE_INT32
    }
}

macro_rules! impl_copy_to_for_numeric {
    ($ty:ty, $info_data_type:expr) => {
        impl CopyTo for $ty {
            type Destination = $ty;

            fn copy_to(
                &self,
                dst: *mut Self::Destination,
                dst_size: *mut libc::size_t,
            ) -> GenTlResult<()> {
                let len = std::mem::size_of::<$ty>();

                if !dst.is_null() {
                    unsafe {
                        if *dst_size < len {
                            return Err(GenTlError::BufferTooSmall);
                        }
                        *dst = *self;
                    }
                }

                unsafe {
                    *dst_size = len;
                }
                Ok(())
            }

            fn info_data_type() -> INFO_DATATYPE {
                $info_data_type
            }
        }
    };
}

impl_copy_to_for_numeric!(i16, INFO_DATATYPE::INFO_DATATYPE_INT16);
impl_copy_to_for_numeric!(u16, INFO_DATATYPE::INFO_DATATYPE_UINT16);
impl_copy_to_for_numeric!(i32, INFO_DATATYPE::INFO_DATATYPE_INT32);
impl_copy_to_for_numeric!(u32, INFO_DATATYPE::INFO_DATATYPE_UINT32);
impl_copy_to_for_numeric!(i64, INFO_DATATYPE::INFO_DATATYPE_INT64);
impl_copy_to_for_numeric!(u64, INFO_DATATYPE::INFO_DATATYPE_UINT64);

fn assert_lib_initialized() -> GenTlResult<()> {
    if *IS_LIB_INITIALIZED.read().unwrap() {
        Ok(())
    } else {
        Err(GenTlError::NotInitialized)
    }
}

gentl_api!(
    no_assert pub fn GCInitLib() -> GenTlResult<()> {
        let mut is_init = IS_LIB_INITIALIZED.write().unwrap();

        if *is_init {
            Err(GenTlError::ResourceInUse)
        } else {
            *is_init = true;
            Ok(())
        }
    }
);

gentl_api!(
    pub fn GCCloseLib() -> GenTlResult<()> {
        let mut is_init = IS_LIB_INITIALIZED.write().unwrap();
        if !*is_init {
            Err(GenTlError::NotInitialized)
        } else {
            *is_init = false;
            Ok(())
        }
    }
);

gentl_api!(
    pub fn CGCGetInfo(
        _iInfoCmd: i32,
        _piType: i32,
        _pBuffer: *mut libc::c_void,
        _piSize: *mut libc::size_t,
    ) -> GenTlResult<()> {
        Err(GenTlError::NotImplemented)
    }
);

gentl_api!(
    pub fn GCGetLastError(
        piErrorCode: *mut GC_ERROR,
        sErrorText: *mut libc::c_char,
        piSize: *mut libc::size_t,
    ) -> GenTlResult<()> {
        let code = match LAST_ERROR.with(|err| {
            let err = err.borrow();
            match &err.err {
                Some(err) => Some((err.into(), format!("{}", err))),
                _ => None,
            }
        }) {
            Some((code, text)) => {
                text.as_str().copy_to(sErrorText, piSize)?;
                code
            }
            None => {
                "No Error".copy_to(sErrorText, piSize)?;
                Ok(()).into()
            }
        };

        unsafe {
            *piErrorCode = code;
        }

        Ok(())
    }
);
