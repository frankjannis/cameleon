use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
};

use cameleon::device::CompressionType;
use cameleon_impl::memory::{prelude::*, MemoryObserver};

use crate::{
    imp::interface::{u3v::U3VInterfaceModule, Interface},
    GenTlResult,
};

use super::{port::*, *};

mod memory;

const NUM_INTERFACE: usize = 1;

pub(crate) struct SystemModule {
    vm: memory::Memory,
    port_info: PortInfo,
    xml_infos: Vec<XmlInfo>,
    system_info: SystemInfo,
    is_opened: bool,

    interfaces: [Box<Mutex<dyn Interface + Send>>; NUM_INTERFACE],
    event_queue: Arc<Mutex<VecDeque<MemoryEvent>>>,
}

impl SystemModule {
    pub(crate) fn new() -> Self {
        let port_info = PortInfo {
            id: memory::GenApi::TLID.into(),
            vendor: memory::GenApi::vendor_name().into(),
            model: memory::GenApi::model_name().into(),
            tl_type: memory::GenApi::TLType::Mixed.into(),
            module_type: ModuleType::System,
            endianness: Endianness::LE,
            access: PortAccess::RW,
            version: memory::GenApi::genapi_version(),
            port_name: memory::GenApi::TLPort.into(),
        };

        let xml_info = XmlInfo {
            location: XmlLocation::RegisterMap {
                address: memory::GenApi::xml_address(),
                size: memory::GenApi::xml_length(),
            },
            schema_version: memory::GenApi::schema_version(),
            file_version: semver::Version::new(
                memory::GenApi::major_version(),
                memory::GenApi::minor_version(),
                memory::GenApi::subminor_version(),
            ),
            sha1_hash: None,
            compressed: CompressionType::Uncompressed,
        };

        let system_info = SystemInfo {
            id: memory::GenApi::TLID.into(),
            vendor: memory::GenApi::vendor_name().into(),
            model: memory::GenApi::model_name().into(),
            version: format!(
                "{}.{}.{}",
                memory::GenApi::major_version(),
                memory::GenApi::minor_version(),
                memory::GenApi::subminor_version()
            ),
            tl_type: memory::GenApi::TLType::Mixed.into(),
            full_path: Self::full_path(),
            display_name: memory::GenApi::tool_tip().into(),
            encoding: CharEncoding::Ascii,
            gentl_version_major: memory::GenApi::GenTLVersionMajor as u32,
            gentl_version_minor: memory::GenApi::GenTLVersionMinor as u32,
        };

        let mut system_module = Self {
            vm: memory::Memory::new(),
            port_info,
            xml_infos: vec![xml_info],
            system_info,
            is_opened: false,

            interfaces: [Box::new(Mutex::new(U3VInterfaceModule::new()))],
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
        };

        system_module.initialize_vm().unwrap();
        system_module
    }

    pub(crate) fn open(&mut self) -> GenTlResult<()> {
        if self.is_opened {
            Err(GenTlError::ResourceInUse)
        } else {
            self.is_opened = true;
            Ok(())
        }
    }

    pub(crate) fn close(&mut self) -> GenTlResult<()> {
        self.assert_open()?;
        for iface in &mut self.interfaces() {
            let _ = iface.lock().unwrap().close();
        }

        Ok(())
    }

    pub(crate) fn is_opened(&self) -> bool {
        self.is_opened
    }

    pub(crate) fn interfaces(&self) -> impl Iterator<Item = &Mutex<dyn Interface + Send>> {
        self.interfaces.iter().map(|iface| iface.as_ref())
    }

    pub(crate) fn interface_of(&self, id: &str) -> Option<&Mutex<dyn Interface + Send>> {
        self.interfaces()
            .find(|iface| iface.lock().unwrap().interface_id() == id)
    }

    pub(crate) fn system_info(&self) -> &SystemInfo {
        &self.system_info
    }

    fn assert_open(&self) -> GenTlResult<()> {
        if self.is_opened {
            Ok(())
        } else {
            Err(GenTlError::NotInitialized)
        }
    }

    fn initialize_vm(&mut self) -> GenTlResult<()> {
        use memory::GenApi;

        let full_path = Self::full_path()
            .into_os_string()
            .into_string()
            .map_err(|e| GenTlError::Error(format!("{:?}", e)))?;
        self.vm.write::<GenApi::TLPathReg>(full_path)?;

        // Initialize registers related to interface.
        self.vm.write::<GenApi::InterfaceSelectorReg>(0)?;
        self.handle_interface_selector_change()?;
        self.vm
            .write::<GenApi::InterfaceSelectorMaxReg>(NUM_INTERFACE as u32 - 1)?;

        // Register observers that trigger events in response to memory write.
        self.register_observers();

        Ok(())
    }

    fn full_path() -> std::path::PathBuf {
        let path = Path::new("../").join(file!());
        std::fs::canonicalize(path).unwrap()
    }

    fn register_observers(&mut self) {
        let interface_update_observer = InterfaceUpdateListRegObserver(self.event_queue.clone());
        self.vm
            .register_observer::<memory::GenApi::InterfaceUpdateListReg, _>(
                interface_update_observer,
            );

        let interface_selector_observer = InterfaceSelectorRegObserver(self.event_queue.clone());
        self.vm
            .register_observer::<memory::GenApi::InterfaceSelectorReg, _>(
                interface_selector_observer,
            );
    }

    fn handle_events(&mut self) -> GenTlResult<()> {
        loop {
            // Drop mutex guard in every iteration to avoid deadlock possibility.
            let event = self.event_queue.lock().unwrap().pop_front();

            match event {
                // We don't implement dynamic update of interfaces.
                Some(MemoryEvent::InterfaceUpdateList) => {}
                Some(MemoryEvent::InterfaceSelector) => self.handle_interface_selector_change()?,
                None => break,
            }
        }

        Ok(())
    }

    fn handle_interface_selector_change(&mut self) -> GenTlResult<()> {
        use memory::GenApi;

        let interface_idx = self.vm.read::<GenApi::InterfaceSelectorReg>()? as usize;

        // Specified interface doesn't exist. In that case, just ignore.
        if interface_idx >= self.interfaces.len() {
            return Err(GenTlError::InvalidIndex);
        }

        let interface = &self.interfaces[interface_idx].lock().unwrap();

        let interface_id = interface.interface_id();
        self.vm
            .write::<GenApi::InterfaceIDReg>(interface_id.into())?;

        macro_rules! byte_array_to_int {
            ($array:expr, $array_size:literal, $result_ty: ty) => {{
                let mut result = $array[0] as $result_ty;
                for i in 1..$array_size {
                    result <<= 8;
                    result += $array[i] as $result_ty;
                }
                result
            }};
        }

        if let Some(mac_addr) = interface.mac_addr() {
            let mac_addr = byte_array_to_int!(mac_addr, 6, u64);
            self.vm
                .write::<GenApi::GevInterfaceMACAddressReg>(mac_addr)?
        }

        if let Some(ip_addr) = interface.ip_addr() {
            let ip_addr = byte_array_to_int!(ip_addr.octets(), 4, u32);
            self.vm
                .write::<GenApi::GevInterfaceDefaultIPAddressReg>(ip_addr)?
        }

        if let Some(subnet_mask) = interface.subnet_mask() {
            let subnet_mask = byte_array_to_int!(subnet_mask.octets(), 4, u32);
            self.vm
                .write::<GenApi::GevInterfaceDefaultSubnetMaskReg>(subnet_mask)?
        }

        if let Some(gateway_addr) = interface.gateway_addr() {
            let gateway_addr = byte_array_to_int!(gateway_addr.octets(), 4, u32);
            self.vm
                .write::<GenApi::GevInterfaceDefaultGatewayReg>(gateway_addr)?
        }

        Ok(())
    }
}

impl Port for SystemModule {
    fn read(&self, address: u64, buf: &mut [u8]) -> GenTlResult<usize> {
        let address = address as usize;
        let len = buf.len();
        let data = self.vm.read_raw(address..address + len)?;
        buf.copy_from_slice(data);
        Ok(len)
    }

    fn write(&mut self, address: u64, data: &[u8]) -> GenTlResult<usize> {
        self.vm.write_raw(address as usize, &data)?;

        self.handle_events()?;

        Ok(data.len())
    }

    fn port_info(&self) -> GenTlResult<&PortInfo> {
        Ok(&self.port_info)
    }

    fn xml_infos(&self) -> GenTlResult<&[XmlInfo]> {
        Ok(&self.xml_infos)
    }
}

pub(crate) struct SystemInfo {
    /// Unique ID identifying a GenTL Producer.
    pub id: String,

    /// GenTL Producer vendor name.
    pub vendor: String,

    /// GenTL Producer model name.
    pub model: String,

    /// GenTL Producer version.
    pub version: String,

    /// Transport layer technology that is supported.
    pub tl_type: TlType,

    /// Full path to this file.
    pub full_path: std::path::PathBuf,

    /// User readable name.
    pub display_name: String,

    /// The char encoding of the GenTL Producer.
    pub encoding: CharEncoding,

    /// Major version number of GenTL Standard Version this Producer complies with.
    pub gentl_version_major: u32,

    /// Minor version number of GenTL Standard Version this Producer complies with.
    pub gentl_version_minor: u32,
}

#[derive(Clone, Copy)]
enum MemoryEvent {
    InterfaceUpdateList,
    InterfaceSelector,
}

#[derive(Clone)]
struct InterfaceUpdateListRegObserver(Arc<Mutex<VecDeque<MemoryEvent>>>);
impl MemoryObserver for InterfaceUpdateListRegObserver {
    fn update(&self) {
        self.0
            .lock()
            .unwrap()
            .push_back(MemoryEvent::InterfaceUpdateList)
    }
}

#[derive(Clone)]
struct InterfaceSelectorRegObserver(Arc<Mutex<VecDeque<MemoryEvent>>>);
impl MemoryObserver for InterfaceSelectorRegObserver {
    fn update(&self) {
        self.0
            .lock()
            .unwrap()
            .push_back(MemoryEvent::InterfaceSelector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory::GenApi;

    #[test]
    fn test_initialize_with_u3v_interface() {
        let system_module = SystemModule::new();
        assert_eq!(
            system_module
                .vm
                .read::<GenApi::InterfaceSelectorReg>()
                .unwrap(),
            0
        );

        let u3v_interface = system_module.interfaces().next().unwrap();
        assert_eq!(
            &system_module.vm.read::<GenApi::InterfaceIDReg>().unwrap(),
            u3v_interface.lock().unwrap().interface_id()
        );
    }
}