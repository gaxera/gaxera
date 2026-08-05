use crate::ObjectType;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootInfo {
    pub magic: u64,
    pub abi_version: u32,
    pub reserved: u32,
}

impl BootInfo {
    pub const MAGIC: u64 = 0x676178657261; // 'gaxera' in hex
    pub const ABI_VERSION: u32 = 1;
}

pub const MAX_BOOTSTRAP_CAPABILITIES: usize = 32;
/// Fixed user virtual address used for the kernel-created bootstrap page.
/// The page is read-only and NX; it contains no kernel pointers or physical
/// addresses.  Keeping the address in the ABI avoids each loader inventing a
/// private convention.
pub const BOOTSTRAP_MANIFEST_VADDR: u64 = 0x0000_7FFF_FFFD_F000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum BootstrapRole {
    SelfAddressSpace = 0,
    SelfThread = 1,
    HeapFactory = 2,
    SupervisorEndpoint = 3,
    ExitNotification = 4,
    BootModule = 5,
    ProcessControl = 6,
    InterruptObject = 7,
    DeviceMemory = 8,
    ServiceEndpoint = 9,
    SelfCapabilitySpace = 10,
    ImageFactory = 11,
    DmaMemory = 12,
    DriverNotification = 13,
}

impl TryFrom<u16> for BootstrapRole {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::SelfAddressSpace),
            1 => Ok(Self::SelfThread),
            2 => Ok(Self::HeapFactory),
            3 => Ok(Self::SupervisorEndpoint),
            4 => Ok(Self::ExitNotification),
            5 => Ok(Self::BootModule),
            6 => Ok(Self::ProcessControl),
            7 => Ok(Self::InterruptObject),
            8 => Ok(Self::DeviceMemory),
            9 => Ok(Self::ServiceEndpoint),
            10 => Ok(Self::SelfCapabilitySpace),
            11 => Ok(Self::ImageFactory),
            12 => Ok(Self::DmaMemory),
            13 => Ok(Self::DriverNotification),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct BootstrapCapability {
    pub role: u16,
    pub object_type: u8,
    pub flags: u8,
    pub rights: u32,
    pub handle: crate::Handle,
    /// Role-specific metadata. For `BootModule` this is the exact byte size
    /// of the module and is never a physical address.
    pub metadata: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct BootstrapManifest {
    pub magic: u64,
    pub abi_version: u32,
    pub header_size: u16,
    pub entry_size: u16,
    pub total_size: u32,
    pub entry_count: u16,
    pub reserved: u16,
    pub process_token: u64,
    pub parent_token: u64,
    pub entries: [BootstrapCapability; MAX_BOOTSTRAP_CAPABILITIES],
}

impl BootstrapManifest {
    pub const MAGIC: u64 = 0x4D42584147; // 'GAXBM'
    pub const ABI_VERSION: u32 = 3;
    pub const HEADER_SIZE: u16 = 40;
    pub const ENTRY_SIZE: u16 = core::mem::size_of::<BootstrapCapability>() as u16;

    pub fn validate(&self) -> Result<(), BootstrapManifestError> {
        if self.magic != Self::MAGIC {
            return Err(BootstrapManifestError::BadMagic);
        }
        if self.abi_version != Self::ABI_VERSION {
            return Err(BootstrapManifestError::UnsupportedVersion);
        }
        if self.header_size != Self::HEADER_SIZE || self.entry_size != Self::ENTRY_SIZE {
            return Err(BootstrapManifestError::BadLayout);
        }
        let count = usize::from(self.entry_count);
        if count > MAX_BOOTSTRAP_CAPABILITIES {
            return Err(BootstrapManifestError::TooManyEntries);
        }
        let expected_size = u32::from(Self::HEADER_SIZE)
            .checked_add(
                u32::from(Self::ENTRY_SIZE)
                    .checked_mul(self.entry_count.into())
                    .ok_or(BootstrapManifestError::BadLength)?,
            )
            .ok_or(BootstrapManifestError::BadLength)?;
        if self.total_size != expected_size
            || usize::try_from(self.total_size).map_err(|_| BootstrapManifestError::BadLength)?
                > core::mem::size_of::<Self>()
        {
            return Err(BootstrapManifestError::BadLength);
        }

        for (index, entry) in self.entries[..count].iter().enumerate() {
            BootstrapRole::try_from(entry.role)
                .map_err(|_| BootstrapManifestError::UnknownRole { index })?;
            if !entry.handle.is_valid() {
                return Err(BootstrapManifestError::InvalidHandle { index });
            }
            let object_type = ObjectType::try_from(u32::from(entry.object_type))
                .map_err(|_| BootstrapManifestError::UnknownObjectType { index })?;
            let _ = object_type;
            // Module and endpoint roles are intentionally repeatable. Their
            // ordinal within the manifest is the stable identity. Singleton
            // roles must not be ambiguous to a consumer.
            let repeatable = matches!(
                BootstrapRole::try_from(entry.role),
                Ok(BootstrapRole::BootModule
                    | BootstrapRole::ServiceEndpoint
                    | BootstrapRole::DeviceMemory)
            );
            if !repeatable {
                for previous in &self.entries[..index] {
                    if previous.role == entry.role {
                        return Err(BootstrapManifestError::DuplicateRole { index });
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapManifestError {
    BadMagic,
    UnsupportedVersion,
    BadLayout,
    TooManyEntries,
    BadLength,
    UnknownRole { index: usize },
    InvalidHandle { index: usize },
    UnknownObjectType { index: usize },
    DuplicateRole { index: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_manifest_layout() {
        assert_eq!(
            core::mem::size_of::<BootstrapCapability>(),
            24,
            "BootstrapCapability must be exactly 16 bytes"
        );
        assert_eq!(
            core::mem::align_of::<BootstrapCapability>(),
            8,
            "BootstrapCapability alignment must be 8"
        );
        assert_eq!(
            core::mem::size_of::<BootstrapManifest>(),
            808,
            "BootstrapManifest must be exactly 808 bytes"
        );
    }

    #[test]
    fn bootstrap_manifest_validation_rejects_malformed_entries() {
        let mut manifest = BootstrapManifest {
            magic: BootstrapManifest::MAGIC,
            abi_version: BootstrapManifest::ABI_VERSION,
            header_size: BootstrapManifest::HEADER_SIZE,
            entry_size: BootstrapManifest::ENTRY_SIZE,
            total_size: u32::from(BootstrapManifest::HEADER_SIZE),
            entry_count: 0,
            reserved: 0,
            process_token: 1,
            parent_token: 2,
            entries: [BootstrapCapability {
                role: 0,
                object_type: 0,
                flags: 0,
                rights: 0,
                handle: crate::Handle::INVALID,
                metadata: 0,
            }; MAX_BOOTSTRAP_CAPABILITIES],
        };
        assert_eq!(manifest.validate(), Ok(()));

        manifest.entry_count = 1;
        manifest.total_size =
            u32::from(BootstrapManifest::HEADER_SIZE) + u32::from(BootstrapManifest::ENTRY_SIZE);
        assert_eq!(
            manifest.validate(),
            Err(BootstrapManifestError::InvalidHandle { index: 0 })
        );
        manifest.entries[0].handle = crate::Handle::from_parts(1, 1);
        assert_eq!(manifest.validate(), Ok(()));
        manifest.entries[1] = manifest.entries[0];
        manifest.entries[1].role = BootstrapRole::BootModule as u16;
        manifest.entry_count = 2;
        manifest.total_size = u32::from(BootstrapManifest::HEADER_SIZE)
            + 2 * u32::from(BootstrapManifest::ENTRY_SIZE);
        assert_eq!(manifest.validate(), Ok(()));

        manifest.entries[1].role = BootstrapRole::SelfAddressSpace as u16;
        assert_eq!(
            manifest.validate(),
            Err(BootstrapManifestError::DuplicateRole { index: 1 })
        );
    }
}
