use alloc::vec::Vec;
use core::fmt;

use x86_64::structures::paging::{FrameAllocator, Size4KiB};

use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::boot::BootContext;
use crate::memory::mapping::ACPI_TABLE_WINDOW;

const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
const XSDT_SIGNATURE: &[u8; 4] = b"XSDT";
const MADT_SIGNATURE: &[u8; 4] = b"APIC";
const RSDP_V1_LENGTH: usize = 20;
const RSDP_V2_MIN_LENGTH: usize = 36;
const SDT_HEADER_LENGTH: usize = 36;
const MADT_FIXED_LENGTH: usize = 44;
const LOCAL_APIC_OVERRIDE_TYPE: u8 = 5;
const LOCAL_APIC_OVERRIDE_LENGTH: usize = 12;
const PAGE_SIZE: u64 = 4096;
const MAX_ACPI_TABLE_LENGTH: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalApicInfo {
    pub physical_address: u64,
    pub used_address_override: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpiError {
    InvalidPhysicalAddress,
    PhysicalAddressOverflow,
    PhysicalReadFailed,
    UnexpectedReadLength,
    TableTooLarge,
    InvalidRsdpSignature,
    UnsupportedRsdpRevision,
    InvalidRsdpLength,
    InvalidChecksum(&'static str),
    MissingXsdt,
    InvalidSdtLength,
    InvalidXsdtSignature,
    InvalidXsdtEntries,
    MissingMadt,
    InvalidMadtSignature,
    MalformedMadtEntry,
    DuplicateLocalApicOverride,
    InvalidLocalApicAddress,
}

impl fmt::Display for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhysicalAddress => f.write_str("ACPI physical address is invalid"),
            Self::PhysicalAddressOverflow => f.write_str("ACPI physical address range overflows"),
            Self::PhysicalReadFailed => f.write_str("failed to read an ACPI physical range"),
            Self::UnexpectedReadLength => {
                f.write_str("ACPI physical reader returned the wrong length")
            }
            Self::TableTooLarge => f.write_str("ACPI table exceeds the Phase 5 size limit"),
            Self::InvalidRsdpSignature => f.write_str("ACPI RSDP signature is invalid"),
            Self::UnsupportedRsdpRevision => {
                f.write_str("Phase 5 requires an ACPI revision 2 or newer RSDP")
            }
            Self::InvalidRsdpLength => f.write_str("ACPI RSDP length is invalid"),
            Self::InvalidChecksum(table) => write!(f, "ACPI {table} checksum is invalid"),
            Self::MissingXsdt => f.write_str("ACPI RSDP does not provide an XSDT"),
            Self::InvalidSdtLength => f.write_str("ACPI SDT length is invalid"),
            Self::InvalidXsdtSignature => f.write_str("ACPI root table is not an XSDT"),
            Self::InvalidXsdtEntries => f.write_str("ACPI XSDT entry array is malformed"),
            Self::MissingMadt => f.write_str("ACPI XSDT does not contain a MADT"),
            Self::InvalidMadtSignature => f.write_str("ACPI MADT signature is invalid"),
            Self::MalformedMadtEntry => f.write_str("ACPI MADT subtable is malformed"),
            Self::DuplicateLocalApicOverride => {
                f.write_str("ACPI MADT contains multiple Local APIC address overrides")
            }
            Self::InvalidLocalApicAddress => f.write_str("ACPI Local APIC address is invalid"),
        }
    }
}

/// Supplies copied bytes from a validated physical range.
///
/// Implementors own the architecture-specific temporary mapping mechanism. The
/// parser never dereferences physical addresses or retains a mapping.
pub trait PhysicalMemoryReader {
    fn read_physical(&mut self, physical_address: u64, length: usize)
    -> Result<Vec<u8>, AcpiError>;
}

/// Discover the Local APIC address through the temporary firmware-table window.
///
/// # Safety
/// Gaxera's post-CR3 hierarchy must be active and the caller must keep
/// interrupts disabled. `page_tables` must be exclusively owned for the full
/// discovery operation, and `allocator` must allocate page-table frames from
/// RAM covered by the active HHDM.
pub unsafe fn discover_from_boot_context<A>(
    context: &BootContext,
    page_tables: &mut KernelPageTables,
    allocator: &mut A,
) -> Result<LocalApicInfo, AcpiError>
where
    A: FrameAllocator<Size4KiB>,
{
    let rsdp = context.rsdp().ok_or(AcpiError::InvalidPhysicalAddress)?;
    let mut reader = PagingReader {
        context,
        page_tables,
        allocator,
    };
    discover_local_apic(&mut reader, rsdp.physical_address)
}

struct PagingReader<'a, A> {
    context: &'a BootContext,
    page_tables: &'a mut KernelPageTables,
    allocator: &'a mut A,
}

impl<A> PhysicalMemoryReader for PagingReader<'_, A>
where
    A: FrameAllocator<Size4KiB>,
{
    fn read_physical(
        &mut self,
        mut physical_address: u64,
        length: usize,
    ) -> Result<Vec<u8>, AcpiError> {
        let mut bytes = Vec::with_capacity(length);
        let mut remaining = length;
        while remaining != 0 {
            let page_physical_address = physical_address & !(PAGE_SIZE - 1);
            let page_offset = usize::try_from(physical_address - page_physical_address)
                .map_err(|_| AcpiError::PhysicalAddressOverflow)?;
            let copied = remaining.min(PAGE_SIZE as usize - page_offset);

            // SAFETY: this reader is the only owner of the fixed window for
            // this operation, maps an ACPI-reclaimable page read-only, copies
            // its bytes, and tears down the mapping before advancing.
            unsafe {
                self.page_tables.map_acpi_table_page(
                    self.context,
                    page_physical_address,
                    self.allocator,
                )
            }
            .map_err(|_| AcpiError::PhysicalReadFailed)?;
            {
                // SAFETY: the mapping above made this exact page present and
                // readable. `copied` stays within the mapped 4 KiB window.
                let source = unsafe {
                    core::slice::from_raw_parts(
                        (ACPI_TABLE_WINDOW as *const u8).add(page_offset),
                        copied,
                    )
                };
                bytes.extend_from_slice(source);
            }
            // SAFETY: the copied slice is out of scope, so no pointer derived
            // from the temporary mapping survives this unmap and TLB flush.
            unsafe { self.page_tables.unmap_acpi_table_page() }
                .map_err(|_| AcpiError::PhysicalReadFailed)?;

            physical_address = physical_address
                .checked_add(u64::try_from(copied).map_err(|_| AcpiError::PhysicalAddressOverflow)?)
                .ok_or(AcpiError::PhysicalAddressOverflow)?;
            remaining -= copied;
        }
        Ok(bytes)
    }
}

/// Validate RSDP -> XSDT -> MADT and return Gaxera-owned Local APIC metadata.
///
/// Phase 5 intentionally supports only ACPI revision 2+ XSDT discovery. It
/// does not interpret AML, retain table mappings, or reclaim ACPI memory.
pub fn discover_local_apic<R>(
    reader: &mut R,
    rsdp_physical_address: u64,
) -> Result<LocalApicInfo, AcpiError>
where
    R: PhysicalMemoryReader,
{
    if rsdp_physical_address == 0 {
        return Err(AcpiError::InvalidPhysicalAddress);
    }

    let rsdp_v1 = read_exact(reader, rsdp_physical_address, RSDP_V1_LENGTH)?;
    if rsdp_v1[..8] != *RSDP_SIGNATURE {
        return Err(AcpiError::InvalidRsdpSignature);
    }
    validate_checksum(&rsdp_v1, "RSDP")?;
    if rsdp_v1[15] < 2 {
        return Err(AcpiError::UnsupportedRsdpRevision);
    }

    let rsdp_prefix = read_exact(reader, rsdp_physical_address, RSDP_V2_MIN_LENGTH)?;
    let rsdp_length =
        usize::try_from(read_u32(&rsdp_prefix, 20)?).map_err(|_| AcpiError::TableTooLarge)?;
    if !(RSDP_V2_MIN_LENGTH..=MAX_ACPI_TABLE_LENGTH).contains(&rsdp_length) {
        return Err(AcpiError::InvalidRsdpLength);
    }
    let rsdp = read_exact(reader, rsdp_physical_address, rsdp_length)?;
    validate_checksum(&rsdp, "extended RSDP")?;
    let xsdt_physical_address = read_u64(&rsdp, 24)?;
    if xsdt_physical_address == 0 {
        return Err(AcpiError::MissingXsdt);
    }

    let xsdt = read_sdt(reader, xsdt_physical_address)?;
    if xsdt[..4] != *XSDT_SIGNATURE {
        return Err(AcpiError::InvalidXsdtSignature);
    }
    let entries = xsdt
        .len()
        .checked_sub(SDT_HEADER_LENGTH)
        .ok_or(AcpiError::InvalidSdtLength)?;
    if !entries.is_multiple_of(8) {
        return Err(AcpiError::InvalidXsdtEntries);
    }

    for offset in (SDT_HEADER_LENGTH..xsdt.len()).step_by(8) {
        let table_physical_address = read_u64(&xsdt, offset)?;
        if table_physical_address == 0 {
            return Err(AcpiError::InvalidPhysicalAddress);
        }
        let table = read_sdt(reader, table_physical_address)?;
        if table[..4] == *MADT_SIGNATURE {
            return parse_madt(&table);
        }
    }

    Err(AcpiError::MissingMadt)
}

fn parse_madt(madt: &[u8]) -> Result<LocalApicInfo, AcpiError> {
    if madt.len() < MADT_FIXED_LENGTH || madt[..4] != *MADT_SIGNATURE {
        return Err(AcpiError::InvalidMadtSignature);
    }

    let header_address = u64::from(read_u32(madt, SDT_HEADER_LENGTH)?);
    let mut selected_address = header_address;
    let mut used_address_override = false;
    let mut offset = MADT_FIXED_LENGTH;
    while offset < madt.len() {
        let remaining = madt.len() - offset;
        if remaining < 2 {
            return Err(AcpiError::MalformedMadtEntry);
        }
        let entry_type = madt[offset];
        let entry_length = usize::from(madt[offset + 1]);
        let end = offset
            .checked_add(entry_length)
            .ok_or(AcpiError::MalformedMadtEntry)?;
        if entry_length < 2 || end > madt.len() {
            return Err(AcpiError::MalformedMadtEntry);
        }
        if entry_type == LOCAL_APIC_OVERRIDE_TYPE {
            if entry_length != LOCAL_APIC_OVERRIDE_LENGTH || read_u16(madt, offset + 2)? != 0 {
                return Err(AcpiError::MalformedMadtEntry);
            }
            if used_address_override {
                return Err(AcpiError::DuplicateLocalApicOverride);
            }
            selected_address = read_u64(madt, offset + 4)?;
            used_address_override = true;
        }
        offset = end;
    }

    if selected_address == 0 || !selected_address.is_multiple_of(PAGE_SIZE) {
        return Err(AcpiError::InvalidLocalApicAddress);
    }
    Ok(LocalApicInfo {
        physical_address: selected_address,
        used_address_override,
    })
}

fn read_sdt<R>(reader: &mut R, physical_address: u64) -> Result<Vec<u8>, AcpiError>
where
    R: PhysicalMemoryReader,
{
    let header = read_exact(reader, physical_address, SDT_HEADER_LENGTH)?;
    let length = usize::try_from(read_u32(&header, 4)?).map_err(|_| AcpiError::TableTooLarge)?;
    if !(SDT_HEADER_LENGTH..=MAX_ACPI_TABLE_LENGTH).contains(&length) {
        return Err(AcpiError::InvalidSdtLength);
    }
    let table = read_exact(reader, physical_address, length)?;
    validate_checksum(&table, "SDT")?;
    Ok(table)
}

fn read_exact<R>(reader: &mut R, physical_address: u64, length: usize) -> Result<Vec<u8>, AcpiError>
where
    R: PhysicalMemoryReader,
{
    let length_u64 = u64::try_from(length).map_err(|_| AcpiError::TableTooLarge)?;
    physical_address
        .checked_add(length_u64)
        .ok_or(AcpiError::PhysicalAddressOverflow)?;
    let bytes = reader.read_physical(physical_address, length)?;
    if bytes.len() != length {
        return Err(AcpiError::UnexpectedReadLength);
    }
    Ok(bytes)
}

fn validate_checksum(bytes: &[u8], table: &'static str) -> Result<(), AcpiError> {
    if bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
        return Err(AcpiError::InvalidChecksum(table));
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, AcpiError> {
    let values = bytes
        .get(
            offset
                ..offset
                    .checked_add(2)
                    .ok_or(AcpiError::PhysicalAddressOverflow)?,
        )
        .ok_or(AcpiError::MalformedMadtEntry)?;
    Ok(u16::from_le_bytes([values[0], values[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, AcpiError> {
    let values = bytes
        .get(
            offset
                ..offset
                    .checked_add(4)
                    .ok_or(AcpiError::PhysicalAddressOverflow)?,
        )
        .ok_or(AcpiError::InvalidSdtLength)?;
    Ok(u32::from_le_bytes([
        values[0], values[1], values[2], values[3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, AcpiError> {
    let values = bytes
        .get(
            offset
                ..offset
                    .checked_add(8)
                    .ok_or(AcpiError::PhysicalAddressOverflow)?,
        )
        .ok_or(AcpiError::InvalidSdtLength)?;
    Ok(u64::from_le_bytes([
        values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7],
    ]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;
    use alloc::vec::Vec;

    use super::{AcpiError, LocalApicInfo, PhysicalMemoryReader, discover_local_apic};

    const RSDP: u64 = 0x1000;
    const XSDT: u64 = 0x2000;
    const MADT: u64 = 0x3000;

    struct MockReader {
        tables: Vec<(u64, Vec<u8>)>,
    }

    impl MockReader {
        fn with_tables(tables: Vec<(u64, Vec<u8>)>) -> Self {
            Self { tables }
        }
    }

    impl PhysicalMemoryReader for MockReader {
        fn read_physical(
            &mut self,
            physical_address: u64,
            length: usize,
        ) -> Result<Vec<u8>, AcpiError> {
            let length = u64::try_from(length).map_err(|_| AcpiError::PhysicalReadFailed)?;
            for (base, table) in &self.tables {
                let Some(offset) = physical_address.checked_sub(*base) else {
                    continue;
                };
                let Some(end) = offset.checked_add(length) else {
                    continue;
                };
                if end <= table.len() as u64 {
                    return Ok(table[offset as usize..end as usize].to_vec());
                }
            }
            Err(AcpiError::PhysicalReadFailed)
        }
    }

    #[test]
    fn discovers_local_apic_from_madt_header() {
        let mut reader = valid_reader(0xfee0_0000, &[]);
        assert_eq!(
            discover_local_apic(&mut reader, RSDP).unwrap(),
            LocalApicInfo {
                physical_address: 0xfee0_0000,
                used_address_override: false,
            }
        );
    }

    #[test]
    fn local_apic_override_supersedes_madt_header() {
        let override_address = 0xfee0_1000_u64;
        let mut entry = vec![5, 12, 0, 0];
        entry.extend_from_slice(&override_address.to_le_bytes());
        let mut reader = valid_reader(0xfee0_0000, &entry);
        assert_eq!(
            discover_local_apic(&mut reader, RSDP).unwrap(),
            LocalApicInfo {
                physical_address: override_address,
                used_address_override: true,
            }
        );
    }

    #[test]
    fn rejects_invalid_rsdp_checksum() {
        let mut reader = valid_reader(0xfee0_0000, &[]);
        reader.tables[0].1[8] ^= 1;
        assert_eq!(
            discover_local_apic(&mut reader, RSDP),
            Err(AcpiError::InvalidChecksum("RSDP"))
        );
    }

    #[test]
    fn rejects_misaligned_xsdt_entry_array() {
        let rsdp = rsdp(XSDT);
        let xsdt = sdt(*b"XSDT", &[0, 0, 0, 0]);
        let madt = madt(0xfee0_0000, &[]);
        let mut reader = MockReader::with_tables(vec![(RSDP, rsdp), (XSDT, xsdt), (MADT, madt)]);
        assert_eq!(
            discover_local_apic(&mut reader, RSDP),
            Err(AcpiError::InvalidXsdtEntries)
        );
    }

    #[test]
    fn rejects_malformed_madt_subtable() {
        let mut reader = valid_reader(0xfee0_0000, &[5, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            discover_local_apic(&mut reader, RSDP),
            Err(AcpiError::MalformedMadtEntry)
        );
    }

    fn valid_reader(local_apic_address: u64, entries: &[u8]) -> MockReader {
        MockReader::with_tables(vec![
            (RSDP, rsdp(XSDT)),
            (XSDT, sdt(*b"XSDT", &MADT.to_le_bytes())),
            (MADT, madt(local_apic_address, entries)),
        ])
    }

    fn rsdp(xsdt_address: u64) -> Vec<u8> {
        let mut bytes = vec![0; 36];
        bytes[..8].copy_from_slice(b"RSD PTR ");
        bytes[15] = 2;
        bytes[20..24].copy_from_slice(&(36_u32).to_le_bytes());
        bytes[24..32].copy_from_slice(&xsdt_address.to_le_bytes());
        set_checksum(&mut bytes[..20], 8);
        set_checksum(&mut bytes, 32);
        bytes
    }

    fn sdt(signature: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 36];
        bytes[..4].copy_from_slice(&signature);
        bytes[4..8].copy_from_slice(&u32::try_from(36 + body.len()).unwrap().to_le_bytes());
        bytes[8] = 1;
        bytes.extend_from_slice(body);
        set_checksum(&mut bytes, 9);
        bytes
    }

    fn madt(local_apic_address: u64, entries: &[u8]) -> Vec<u8> {
        let mut bytes = sdt(*b"APIC", &[0; 8]);
        bytes[36..40].copy_from_slice(&(local_apic_address as u32).to_le_bytes());
        bytes.extend_from_slice(entries);
        let length = u32::try_from(bytes.len()).unwrap();
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        bytes[9] = 0;
        set_checksum(&mut bytes, 9);
        bytes
    }

    fn set_checksum(bytes: &mut [u8], checksum_offset: usize) {
        bytes[checksum_offset] = 0;
        let sum = bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
        bytes[checksum_offset] = 0_u8.wrapping_sub(sum);
    }
}
