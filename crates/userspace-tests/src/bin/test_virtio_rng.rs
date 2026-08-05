#![no_std]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code, unused_imports))]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

extern crate alloc;

use core::sync::atomic::{Ordering, fence};
use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, InterruptOp, ObjectType, Rights};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

const COMMON_CFG: u8 = 1;
const NOTIFY_CFG: u8 = 2;
const ISR_CFG: u8 = 3;

const COMMON_VADDR: u64 = 0x4000_0000;
const NOTIFY_VADDR: u64 = 0x4000_1000;
const ISR_VADDR: u64 = 0x4000_2000;
const DMA_VADDR: u64 = 0x5000_0000;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
    // SAFETY: The kernel supplies a validated bootstrap manifest pointer and
    // length for this process entry.
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

    let factory = match find(manifest, BootstrapRole::HeapFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let aspace = match find(manifest, BootstrapRole::SelfAddressSpace) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: VIRTIO_RNG_DRIVER_START\n");

    let common = match find_region(manifest, COMMON_CFG) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let notify = match find_region(manifest, NOTIFY_CFG) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let isr = match find_region(manifest, ISR_CFG) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };

    let common_map =
        match syscall::map_memory(aspace, common, COMMON_VADDR, Rights::READ | Rights::WRITE) {
            Ok(handle) => handle,
            Err(_) => syscall::exit(gaxera_abi::status::MAPPING_COLLISION),
        };
    let notify_map =
        match syscall::map_memory(aspace, notify, NOTIFY_VADDR, Rights::READ | Rights::WRITE) {
            Ok(handle) => handle,
            Err(_) => syscall::exit(gaxera_abi::status::MAPPING_COLLISION),
        };
    let isr_map = match syscall::map_memory(aspace, isr, ISR_VADDR, Rights::READ | Rights::WRITE) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::MAPPING_COLLISION),
    };

    let dma = match syscall::factory_create_contiguous_frame(factory, 16 * 1024) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let (dma_phys, _) = match syscall::contiguous_frame_info(dma) {
        Ok(info) => info,
        Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let dma_map = match syscall::map_memory(aspace, dma, DMA_VADDR, Rights::READ | Rights::WRITE) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::MAPPING_COLLISION),
    };

    let irq = match find(manifest, BootstrapRole::InterruptObject) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let notification = match syscall::factory_create(factory, ObjectType::Notification) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    if syscall::interrupt_control_with_arg(irq, InterruptOp::BindNotification, notification.raw())
        .is_err()
    {
        syscall::exit(gaxera_abi::status::RIGHTS_DENIED);
    }

    // Reset and negotiate the minimal VirtIO 1.0 feature set for RNG.
    write_u8(COMMON_VADDR, 0x14, 0);
    write_u8(COMMON_VADDR, 0x14, 1); // ACKNOWLEDGE
    write_u8(COMMON_VADDR, 0x14, 3); // DRIVER
    write_u32(COMMON_VADDR, 0x00, 0);
    write_u32(COMMON_VADDR, 0x08, 0);
    write_u8(COMMON_VADDR, 0x14, 11); // FEATURES_OK
    if read_u8(COMMON_VADDR, 0x14) & 8 == 0 {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }

    write_u16(COMMON_VADDR, 0x16, 0); // queue_select
    let queue_size = read_u16(COMMON_VADDR, 0x18).min(8);
    if queue_size == 0 {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    write_u64(COMMON_VADDR, 0x20, dma_phys);
    write_u64(COMMON_VADDR, 0x28, dma_phys + 128);
    write_u64(COMMON_VADDR, 0x30, dma_phys + 4096);
    write_u16(COMMON_VADDR, 0x1c, 1); // queue_enable
    write_u8(COMMON_VADDR, 0x14, 15); // DRIVER_OK

    // One writable descriptor points at a DMA buffer inside the same
    // capability-backed contiguous frame. The device completion, not a timer,
    // is what wakes the Ring-3 thread.
    let data_phys = dma_phys + 8192;
    write_u64(DMA_VADDR, 0, data_phys);
    write_u32(DMA_VADDR, 8, 64);
    write_u16(DMA_VADDR, 12, 2); // VIRTQ_DESC_F_WRITE
    write_u16(DMA_VADDR, 14, 0);
    write_u16(DMA_VADDR, 128, 0); // avail flags
    write_u16(DMA_VADDR, 132, 0); // avail ring[0]
    write_u16(DMA_VADDR, 130, 1); // avail idx
    fence(Ordering::Release);

    if syscall::interrupt_control(irq, InterruptOp::Unmask).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let notify_offset = read_u16(COMMON_VADDR, 0x1e);
    let notify_multiplier = region_multiplier(find_metadata(manifest, NOTIFY_CFG));
    let notify_address =
        NOTIFY_VADDR.wrapping_add(u64::from(notify_offset) * u64::from(notify_multiplier));
    write_u16(notify_address, 0, 0);

    if syscall::wait_notification(notification).is_err() {
        syscall::exit(gaxera_abi::status::TIMED_OUT);
    }
    fence(Ordering::Acquire);
    let _isr_status = read_u8(ISR_VADDR, region_offset(find_metadata(manifest, ISR_CFG)));
    let used_idx = read_u16(DMA_VADDR, 4096 + 2);
    if used_idx == 0 || syscall::interrupt_control(irq, InterruptOp::Ack).is_err() {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }

    let _ = syscall::interrupt_control(irq, InterruptOp::Mask);
    let _ = syscall::unmap_memory(dma_map);
    let _ = syscall::unmap_memory(isr_map);
    let _ = syscall::unmap_memory(notify_map);
    let _ = syscall::unmap_memory(common_map);
    let _ = syscall::delete_handle(notification);
    let _ = syscall::delete_handle(irq);
    let _ = syscall::delete_handle(dma);
    let _ = syscall::delete_handle(isr);
    let _ = syscall::delete_handle(notify);
    let _ = syscall::delete_handle(common);
    let _ = syscall::debug_console_write(console, "GAXERA: VIRTIO_RNG_IRQ_SUCCESS\n");
    syscall::exit(0);
}

fn find(manifest: &BootstrapManifest, role: BootstrapRole) -> Option<Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == role as u16)
        .map(|entry| entry.handle)
}

fn find_region(manifest: &BootstrapManifest, cfg_type: u8) -> Option<Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| {
            entry.role == BootstrapRole::DeviceMemory as u16
                && (entry.metadata & 0xFF) == u64::from(cfg_type)
        })
        .map(|entry| entry.handle)
}

fn find_metadata(manifest: &BootstrapManifest, cfg_type: u8) -> u64 {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| {
            entry.role == BootstrapRole::DeviceMemory as u16
                && (entry.metadata & 0xFF) == u64::from(cfg_type)
        })
        .map(|entry| entry.metadata)
        .unwrap_or(0)
}

fn region_offset(metadata: u64) -> u64 {
    (metadata >> 16) & 0xFFFF_FFFF
}

fn region_multiplier(metadata: u64) -> u32 {
    let multiplier = (metadata >> 48) as u32;
    if multiplier == 0 { 1 } else { multiplier }
}

fn write_u8(base: u64, offset: u64, value: u8) {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *mut u8).write_volatile(value) };
}

fn read_u8(base: u64, offset: u64) -> u8 {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *const u8).read_volatile() }
}

fn write_u16(base: u64, offset: u64, value: u16) {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *mut u16).write_volatile(value) };
}

fn read_u16(base: u64, offset: u64) -> u16 {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *const u16).read_volatile() }
}

fn write_u32(base: u64, offset: u64, value: u32) {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *mut u32).write_volatile(value) };
}

fn write_u64(base: u64, offset: u64, value: u64) {
    // SAFETY: base is a capability-backed, mapped VirtIO MMIO window.
    unsafe { ((base + offset) as *mut u64).write_volatile(value) };
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(0xDEAD_0070);
}
