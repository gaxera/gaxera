#![no_std]
#![cfg_attr(not(test), no_main)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

#[cfg(test)]
extern crate std;

#[cfg(not(test))]
use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
#[cfg(not(test))]
use gaxera_abi::{Handle, ProcessControlOp, Rights};
#[cfg(not(test))]
use libgaxera::allocator::UserspaceAllocator;
#[cfg(not(test))]
use libgaxera::process::ProcessBuilder;
#[cfg(not(test))]
use libgaxera::syscall;

#[repr(align(8))]
struct AlignedImage([u8; 4096]);

struct Code {
    bytes: [u8; 2400],
    len: usize,
    fail_jumps: [usize; 32],
    fail_targets: [usize; 32],
    fail_count: usize,
}

impl Code {
    const fn new() -> Self {
        Self {
            bytes: [0; 2400],
            len: 0,
            fail_jumps: [0; 32],
            fail_targets: [0; 32],
            fail_count: 0,
        }
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.bytes[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }

    fn emit_u64(&mut self, value: u64) {
        self.emit(&value.to_le_bytes());
    }

    fn emit_fail_if_nonzero(&mut self) {
        self.emit(&[0x48, 0x85, 0xc0]); // test rax,rax
        let at = self.len;
        self.emit(&[0x0f, 0x85, 0, 0, 0, 0]); // jne fail
        self.fail_jumps[self.fail_count] = at;
        self.fail_count += 1;
    }

    fn emit_fail_if_zero(&mut self) {
        let at = self.len;
        self.emit(&[0x0f, 0x84, 0, 0, 0, 0]); // je fail
        self.fail_jumps[self.fail_count] = at;
        self.fail_count += 1;
    }

    fn emit_exit(&mut self, status: u64) {
        self.emit(&[0x31, 0xff]); // xor edi,edi
        self.emit(&[0xbe, 99, 0, 0, 0]); // ExitProcess operation
        self.emit(&[0x48, 0xba]);
        self.emit_u64(status);
        self.emit(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05, 0xeb, 0xfe]);
    }

    fn patch_fail_jumps(&mut self) {
        for index in 0..self.fail_count {
            let at = self.fail_jumps[index];
            let target = self.fail_targets[index];
            let displacement = (target as isize - (at + 6) as isize) as i32;
            self.bytes[at + 2..at + 6].copy_from_slice(&displacement.to_le_bytes());
        }
    }

    fn patch_rel32(&mut self, at: usize, target: usize) {
        let displacement = (target as isize - (at + 6) as isize) as i32;
        self.bytes[at + 2..at + 6].copy_from_slice(&displacement.to_le_bytes());
    }

    fn patch_jmp(&mut self, at: usize, target: usize) {
        let displacement = (target as isize - (at + 5) as isize) as i32;
        self.bytes[at + 1..at + 5].copy_from_slice(&displacement.to_le_bytes());
    }
}

fn child_driver_image(exit_status: u64) -> AlignedImage {
    let mut image = AlignedImage([0; 4096]);
    let bytes = &mut image.0;

    bytes[0..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&0x3eu16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[24..32].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
    bytes[48..50].copy_from_slice(&64u16.to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&1u16.to_le_bytes());

    let ph = 64;
    bytes[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
    bytes[ph + 4..ph + 8].copy_from_slice(&5u32.to_le_bytes());
    bytes[ph + 8..ph + 16].copy_from_slice(&120u64.to_le_bytes());
    bytes[ph + 16..ph + 24].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[ph + 24..ph + 32].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[ph + 48..ph + 56].copy_from_slice(&0x1000u64.to_le_bytes());

    let mut code = Code::new();
    macro_rules! emit {
        ($part:expr) => {
            code.emit($part)
        };
    }

    // Parse the versioned manifest. Repeated DeviceMemory entries are
    // selected by ordinal: common, notify, then ISR. No raw slot is assumed.
    emit!(&[0x31, 0xc9]); // xor ecx,ecx
    emit!(&[0x45, 0x31, 0xe4]); // DMA frame = 0
    emit!(&[0x45, 0x31, 0xed]); // address space = 0
    emit!(&[0x45, 0x31, 0xf6]); // interrupt = 0
    emit!(&[0x45, 0x31, 0xff]); // device ordinal = 0
    emit!(&[0x45, 0x31, 0xdb]); // console = 0
    emit!(&[0x31, 0xdb]); // common = 0
    emit!(&[0x31, 0xed]); // notify = 0
    emit!(&[0x45, 0x31, 0xc0]); // isr = 0
    emit!(&[0x45, 0x31, 0xc9]); // driver notification = 0

    let scan_loop = code.len;
    emit!(&[0x66, 0x3b, 0x4f, 0x14]); // cmp cx,[rdi+entry_count]
    let scan_done = code.len;
    emit!(&[0x0f, 0x83, 0, 0, 0, 0]); // jae scan_done
    emit!(&[0x0f, 0xb7, 0xc1]); // movzx eax,cx
    emit!(&[0x69, 0xc0, 24, 0, 0, 0]); // imul eax,eax,24
    emit!(&[0x48, 0x01, 0xf8]); // add rax,rdi
    emit!(&[0x48, 0x83, 0xc0, 40]); // add rax,40
    emit!(&[0x0f, 0xb7, 0x10]); // movzx edx,[rax]

    emit!(&[0x83, 0xfa, 12]); // DmaMemory
    let not_factory = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x4c, 0x8b, 0x60, 8]); // r12=[rax+8]
    let scan_next_factory = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let check_aspace = code.len;
    emit!(&[0x83, 0xfa, 0]);
    let not_aspace = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x4c, 0x8b, 0x68, 8]); // r13=[rax+8]
    let scan_next_aspace = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let check_irq = code.len;
    emit!(&[0x83, 0xfa, 7]);
    let not_irq = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x4c, 0x8b, 0x70, 8]); // r14=[rax+8]
    let scan_next_irq = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let check_driver_notification = code.len;
    emit!(&[0x83, 0xfa, 13]);
    let not_driver_notification = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x4c, 0x8b, 0x48, 8]); // driver notification
    let scan_next_driver_notification = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let check_console = code.len;
    emit!(&[0x83, 0xfa, 9]);
    let not_console = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x4c, 0x8b, 0x58, 8]); // console
    let scan_next_console = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let check_device = code.len;
    emit!(&[0x83, 0xfa, 8]);
    let scan_next_other = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x45, 0x85, 0xff]); // ordinal == 0?
    let device_not_first = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x48, 0x8b, 0x58, 8]); // common
    let device_done = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);
    let device_second = code.len;
    emit!(&[0x41, 0x83, 0xff, 1]);
    let device_not_second = code.len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]);
    emit!(&[0x48, 0x8b, 0x68, 8]); // notify
    let device_second_done = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);
    let device_third = code.len;
    emit!(&[0x4c, 0x8b, 0x40, 8]); // isr
    let device_increment = code.len;
    emit!(&[0x41, 0xff, 0xc7]); // ++ordinal
    let entry_increment = code.len;
    emit!(&[0x66, 0xff, 0xc1]); // ++entry index
    let scan_back = code.len;
    emit!(&[0xe9, 0, 0, 0, 0]);

    let scan_finished = code.len;
    for register in [[0x4d, 0x85, 0xe4], [0x4d, 0x85, 0xed], [0x4d, 0x85, 0xf6]] {
        emit!(&register);
        code.emit_fail_if_zero();
    }
    emit!(&[0x48, 0x85, 0xdb]); // common
    code.emit_fail_if_zero();
    emit!(&[0x48, 0x85, 0xed]); // notify
    code.emit_fail_if_zero();
    emit!(&[0x4d, 0x85, 0xc0]); // isr
    code.emit_fail_if_zero();
    emit!(&[0x4d, 0x85, 0xdb]); // console
    code.emit_fail_if_zero();
    emit!(&[0x4d, 0x85, 0xc9]); // driver notification
    code.emit_fail_if_zero();
    emit!(&[0x4d, 0x89, 0xe7]); // r15 = DMA handle
    emit!(&[0x4d, 0x89, 0xcc]); // r12 = notification handle
    emit!(&[0x4c, 0x89, 0x5c, 0x24, 0xf8]); // save console handle below rsp

    // Map common, notify, and ISR windows into fixed private addresses.
    for (handle, vaddr) in [
        ("r8", 0x4000_2000u64),
        ("rbx", 0x4000_0000u64),
        ("rbp", 0x4000_1000),
    ] {
        emit!(&[0x4c, 0x89, 0xef]); // mov rdi,r13
        if handle == "rbx" {
            emit!(&[0x48, 0x89, 0xda]);
        } else if handle == "rbp" {
            emit!(&[0x48, 0x89, 0xea]);
        } else {
            emit!(&[0x4c, 0x89, 0xc2]);
        }
        emit!(&[0xbe, 1, 0, 0, 0]);
        emit!(&[0x49, 0xba]);
        code.emit_u64(vaddr);
        emit!(&[0x41, 0xb8, 3, 0, 0, 0]);
        emit!(&[0x45, 0x31, 0xc9, 0xb8, 10, 0, 0, 0, 0x0f, 0x05]);
        code.emit_fail_if_nonzero();
    }

    // Map the DMA frame at 0x50000000.
    emit!(&[0x4c, 0x89, 0xef, 0x4c, 0x89, 0xfa, 0xbe, 1, 0, 0, 0]);
    emit!(&[0x49, 0xba]);
    code.emit_u64(0x5000_0000);
    emit!(&[
        0x41, 0xb8, 3, 0, 0, 0, 0x45, 0x31, 0xc9, 0xb8, 10, 0, 0, 0, 0x0f, 0x05
    ]);
    code.emit_fail_if_nonzero();

    // Query the supervisor-provided DMA frame's physical base.
    emit!(&[0x4c, 0x89, 0xff, 0xbe, 2, 0, 0, 0, 0x31, 0xd2]);
    emit!(&[0x45, 0x31, 0xd2, 0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9]);
    emit!(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05]);
    code.emit_fail_if_nonzero();
    emit!(&[0x49, 0x89, 0xd7]); // r15 = physical base

    // Bind the supervisor-provided Notification to the inherited InterruptObject.
    emit!(&[0x4c, 0x89, 0xf7, 0xbe, 15, 0, 0, 0, 0xba, 1, 0, 0, 0]);
    emit!(&[0x4d, 0x89, 0xe2, 0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9]);
    emit!(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05]);
    code.emit_fail_if_nonzero();

    // Minimal VirtIO RNG initialization and one writable request.
    for value in [0u8, 1, 3] {
        emit!(&[0x48, 0xb8]);
        code.emit_u64(0x4000_0000 + 0x14);
        emit!(&[0xc6, 0x00, value]);
    }
    for offset in [0x00u64, 0x08] {
        emit!(&[0x48, 0xb8]);
        code.emit_u64(0x4000_0000 + offset);
        emit!(&[0xc7, 0x00, 0, 0, 0, 0]);
    }
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x14);
    emit!(&[0xc6, 0x00, 11]);
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x16);
    emit!(&[0x66, 0xc7, 0x00, 0, 0]);
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x18);
    emit!(&[0x0f, 0xb7, 0x00]);
    emit!(&[0x66, 0x85, 0xc0]);
    code.emit_fail_if_zero();
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x20);
    emit!(&[0x4c, 0x89, 0x38]);
    emit!(&[0x4c, 0x89, 0xf8, 0x48, 0x05, 128, 0, 0, 0]);
    emit!(&[0x48, 0x89, 0xc2]); // rdx = dma_phys + 128
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x28);
    emit!(&[0x48, 0x89, 0x10]);
    emit!(&[0x4c, 0x89, 0xf8, 0x48, 0x05, 0, 16, 0, 0]);
    emit!(&[0x48, 0x89, 0xc2]); // rdx = dma_phys + 4096
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x30);
    emit!(&[0x48, 0x89, 0x10]);
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x1c);
    emit!(&[0x66, 0xc7, 0x00, 1, 0]);
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x14);
    emit!(&[0xc6, 0x00, 15]);
    // Descriptor and available-ring publication is ordered before notify.
    emit!(&[0x4c, 0x89, 0xf8, 0x48, 0x05, 0, 32, 0, 0]);
    emit!(&[0x48, 0x89, 0xc2]); // rdx = data_phys
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x5000_0000);
    emit!(&[0x48, 0x89, 0x10]);
    for (offset, bytes) in [
        (8u64, &[0xc7, 0x00, 64, 0, 0, 0][..]),
        (12, &[0x66, 0xc7, 0, 2, 0][..]),
        (14, &[0x66, 0xc7, 0, 0, 0][..]),
        (128, &[0x66, 0xc7, 0, 0, 0][..]),
        (132, &[0x66, 0xc7, 0, 0, 0][..]),
        (130, &[0x66, 0xc7, 0, 1, 0][..]),
    ] {
        emit!(&[0x48, 0xb8]);
        code.emit_u64(0x5000_0000 + offset);
        emit!(&bytes);
    }
    emit!(&[0x0f, 0xae, 0xf0]); // mfence: publish DMA writes
    emit!(&[0x4c, 0x89, 0xf7, 0xbe, 15, 0, 0, 0, 0xba, 3, 0, 0, 0]);
    emit!(&[
        0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9, 0xb8, 10, 0, 0, 0, 0x0f, 0x05
    ]);
    code.emit_fail_if_nonzero();
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x4000_0000 + 0x1e);
    emit!(&[0x0f, 0xb7, 0x00]);
    emit!(&[0x48, 0xc1, 0xe0, 2]); // notify_off_multiplier = 4 on QEMU PCI
    emit!(&[0x48, 0xba]);
    code.emit_u64(0x4000_1000);
    emit!(&[0x48, 0x01, 0xd0]);
    emit!(&[0x0f, 0xae, 0xf0]); // order descriptor publication before notify
    emit!(&[0x66, 0xc7, 0x00, 0, 0]);
    emit!(&[
        0x31, 0xff, 0xbe, 0, 0, 0, 0, 0x48, 0x31, 0xd2, 0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9, 0xb8,
        10, 0, 0, 0, 0x0f, 0x05
    ]);
    code.emit_fail_if_nonzero();

    // Wait for the real device completion, consume the used ring, ACK, rearm.
    emit!(&[
        0x4c, 0x89, 0xe7, 0xbe, 16, 0, 0, 0, 0x45, 0x31, 0xd2, 0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9
    ]);
    emit!(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05]);
    emit!(&[0x48, 0xb8]);
    code.emit_u64(0x5000_0000 + 4098);
    emit!(&[0x0f, 0xb7, 0x00]);
    emit!(&[0x66, 0x85, 0xc0]);
    code.emit_fail_if_zero();
    // ACK completes the level-triggered delivery transaction and rearms the
    // controller. A second Unmask would be redundant and is intentionally
    // not part of the driver contract.
    emit!(&[0x4c, 0x89, 0xf7, 0xbe, 15, 0, 0, 0, 0xba, 4, 0, 0, 0]);
    emit!(&[
        0x45, 0x31, 0xd2, 0x45, 0x31, 0xc0, 0x45, 0x31, 0xc9, 0xb8, 10, 0, 0, 0, 0x0f, 0x05
    ]);
    code.emit_fail_if_nonzero();

    code.emit_exit(exit_status);
    for index in 0..code.fail_count {
        code.fail_targets[index] = code.len;
        code.emit_exit(0xe0 + index as u64);
    }
    code.patch_fail_jumps();
    code.patch_rel32(scan_done, scan_finished);
    code.patch_rel32(not_factory, check_aspace);
    code.patch_jmp(scan_next_factory, entry_increment);
    code.patch_rel32(not_aspace, check_irq);
    code.patch_jmp(scan_next_aspace, entry_increment);
    code.patch_rel32(not_irq, check_driver_notification);
    code.patch_rel32(not_driver_notification, check_console);
    code.patch_rel32(not_console, check_device);
    code.patch_jmp(scan_next_console, entry_increment);
    code.patch_jmp(scan_next_driver_notification, entry_increment);
    code.patch_jmp(scan_next_irq, entry_increment);
    code.patch_rel32(scan_next_other, entry_increment);
    code.patch_rel32(device_not_first, device_second);
    code.patch_jmp(device_done, device_increment);
    code.patch_rel32(device_not_second, device_third);
    code.patch_jmp(device_second_done, device_increment);
    code.patch_jmp(scan_back, scan_loop);
    bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
    bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
    bytes[96..104].copy_from_slice(&(code.len as u64).to_le_bytes());
    bytes[104..112].copy_from_slice(&(code.len as u64).to_le_bytes());
    bytes[120..120 + code.len].copy_from_slice(&code.bytes[..code.len]);
    image
}

#[cfg(not(test))]
fn wait_for_zombie(process: Handle) -> Result<u64, ()> {
    for _ in 0..10_000 {
        if let Ok((state, status)) = syscall::process_query(process)
            && state == 6
        {
            return Ok(status);
        }
        syscall::yield_now().map_err(|_| ())?;
    }
    Err(())
}

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
    // SAFETY: The kernel passes a validated, process-owned bootstrap manifest
    // pointer and bounded length for this Ring-3 entry point.
    let manifest = match unsafe {
        libgaxera::entry::initialize_userspace_allocator(
            manifest_pointer,
            manifest_length,
            &ALLOCATOR,
        )
    } {
        Ok(manifest) => manifest,
        Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let find = |role: BootstrapRole| {
        manifest.entries[..usize::from(manifest.entry_count)]
            .iter()
            .find(|entry| entry.role == role as u16)
            .map(|entry| entry.handle)
    };
    let factory = match find(BootstrapRole::HeapFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let aspace = match find(BootstrapRole::SelfAddressSpace) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let image_factory = match find(BootstrapRole::ImageFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, gaxera_abi::ObjectType::DebugConsole) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let mut device_memory = [Handle::INVALID; 3];
    let mut device_count = 0usize;
    let irq = manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == BootstrapRole::InterruptObject as u16)
        .map(|entry| entry.handle);
    for entry in &manifest.entries[..usize::from(manifest.entry_count)] {
        if entry.role == BootstrapRole::DeviceMemory as u16 && device_count < 3 {
            device_memory[device_count] = entry.handle;
            device_count += 1;
        }
    }
    let irq = match irq {
        Some(handle) if device_count == 3 => handle,
        _ => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };

    let start_driver = |exit_status| {
        let image = child_driver_image(exit_status);
        let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_IMAGE_BUILT\n");
        let dma = match syscall::factory_create_contiguous_frame(factory, 16 * 1024) {
            Ok(handle) => handle,
            Err(_) => return Err(()),
        };
        let notification =
            match syscall::factory_create(factory, gaxera_abi::ObjectType::Notification) {
                Ok(handle) => handle,
                Err(_) => {
                    let _ = syscall::delete_handle(dma);
                    return Err(());
                }
            };
        let mut builder =
            ProcessBuilder::new(factory, aspace, &image.0).with_image_factory(image_factory);
        for mapping in device_memory {
            builder = match builder.install_capability(
                BootstrapRole::DeviceMemory as u16,
                mapping,
                Rights::MAP | Rights::READ | Rights::WRITE,
            ) {
                Ok(builder) => builder,
                Err(_) => {
                    let _ = syscall::delete_handle(dma);
                    let _ = syscall::delete_handle(notification);
                    return Err(());
                }
            };
        }
        builder = match builder.install_capability(
            BootstrapRole::InterruptObject as u16,
            irq,
            Rights::INTERRUPT,
        ) {
            Ok(builder) => builder,
            Err(_) => {
                let _ = syscall::delete_handle(dma);
                let _ = syscall::delete_handle(notification);
                return Err(());
            }
        };
        builder = match builder.install_capability(
            BootstrapRole::DmaMemory as u16,
            dma,
            Rights::MAP | Rights::READ | Rights::WRITE,
        ) {
            Ok(builder) => builder,
            Err(_) => {
                let _ = syscall::delete_handle(dma);
                let _ = syscall::delete_handle(notification);
                return Err(());
            }
        };
        builder = match builder.install_capability(
            BootstrapRole::DriverNotification as u16,
            notification,
            Rights::READ | Rights::SIGNAL | Rights::WAIT,
        ) {
            Ok(builder) => builder,
            Err(_) => {
                let _ = syscall::delete_handle(dma);
                let _ = syscall::delete_handle(notification);
                return Err(());
            }
        };
        builder = match builder.install_capability(
            BootstrapRole::ServiceEndpoint as u16,
            console,
            Rights::WRITE,
        ) {
            Ok(builder) => builder,
            Err(_) => return Err(()),
        };
        builder
            .spawn()
            .map(|process| (process, dma, notification))
            .map_err(|_| ())
    };

    let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_SUPERVISOR_READY\n");
    let (first, first_dma, first_notification) = match start_driver(0xdead) {
        Ok(resources) => resources,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_START_BAD\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR)
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_STARTED\n");
    let first_status = match wait_for_zombie(first) {
        Ok(status) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_REAPABLE\n");
            Ok(status)
        }
        Err(()) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_WAIT_TIMEOUT\n");
            Err(())
        }
    };
    if first_status != Ok(0xdead) {
        if first_status == Ok(0xdead_0000) {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_CHILD_PANIC\n");
        } else if let Ok(status @ 0xe0..=0xff) = first_status {
            let stage = match status {
                0xe0 => "00",
                0xe1 => "01",
                0xe2 => "02",
                0xe3 => "03",
                0xe4 => "04",
                0xe5 => "05",
                0xe6 => "06",
                0xe7 => "07",
                0xe8 => "08",
                0xe9 => "09",
                0xea => "10",
                0xeb => "11",
                0xec => "12",
                0xed => "13",
                0xee => "14",
                0xef => "15",
                0xf0 => "16",
                0xf1 => "17",
                0xf2 => "18",
                0xf3 => "19",
                0xf4 => "20",
                0xf5 => "21",
                0xf6 => "22",
                0xf7 => "23",
                0xf8 => "24",
                0xf9 => "25",
                0xfa => "26",
                0xfb => "27",
                0xfc => "28",
                0xfd => "29",
                0xfe => "30",
                _ => "31",
            };
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_CHILD_FAIL_STAGE_");
            let _ = syscall::debug_console_write(console, stage);
            let _ = syscall::debug_console_write(console, "\n");
        } else {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_CHILD_OTHER_STATUS\n");
        }
        let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_STATUS_BAD\n");
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    if syscall::process_control(first, ProcessControlOp::Reap, 0, 0, 0).is_err() {
        let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_FIRST_REAP_BAD\n");
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let _ = syscall::delete_handle(first_dma);
    let _ = syscall::delete_handle(first_notification);

    let (second, second_dma, second_notification) = match start_driver(0) {
        Ok(resources) => resources,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_SECOND_START_BAD\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR)
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_SECOND_STARTED\n");
    if second == first
        || wait_for_zombie(second).is_err()
        || syscall::process_control(second, ProcessControlOp::Reap, 0, 0, 0).is_err()
    {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let _ = syscall::delete_handle(second_dma);
    let _ = syscall::delete_handle(second_notification);
    let _ = syscall::debug_console_write(console, "GAXERA: DRIVER_CRASH_RESTART_SUCCESS\n");
    syscall::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    libgaxera::syscall::exit(0xdead_0000);
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_driver_image_fits() {
        let image = super::child_driver_image(0);
        assert_eq!(&image.0[0..4], b"\x7fELF");
    }
}
