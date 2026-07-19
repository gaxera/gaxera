#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

#[cfg(any(
    all(feature = "panic-test", feature = "test-boot"),
    all(feature = "panic-test", feature = "test-apic-timer"),
    all(
        feature = "panic-test",
        any(
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-apic-timer",
            feature = "test-preemption",
        )
    ),
    all(
        feature = "test-boot",
        any(
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-apic-timer",
            feature = "test-preemption",
        )
    ),
    all(
        feature = "test-memory",
        any(
            feature = "panic-test",
            feature = "test-boot",
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-heap-guard",
            feature = "test-apic-timer",
            feature = "test-preemption",
        )
    ),
    all(
        feature = "test-heap-guard",
        any(
            feature = "panic-test",
            feature = "test-boot",
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-apic-timer",
            feature = "test-preemption",
        )
    ),
    all(
        feature = "test-apic-timer",
        any(
            feature = "panic-test",
            feature = "test-boot",
            feature = "test-memory",
            feature = "test-heap-guard",
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-preemption",
        )
    ),
    all(
        feature = "test-preemption",
        any(
            feature = "panic-test",
            feature = "test-boot",
            feature = "test-memory",
            feature = "test-heap-guard",
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
            feature = "test-apic-timer",
        )
    ),
))]
compile_error!("exactly one Gaxera QEMU test profile may be enabled");

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem;
use core::panic::PanicInfo;
use kernel::memory::mapping::{HEAP_LOWER_GUARD, HEAP_SIZE, HEAP_START};
use kernel::memory::physical::{
    BootstrapFrameAllocator, PAGE_SIZE, SegmentedBitmapFrameAllocator, initialize_global_allocator,
};
use kernel::{arch, framebuffer, println, serial};

/// Rust entry after the assembly trampoline replaces Limine's stack.
///
/// # Safety
/// The trampoline establishes a 16-byte-aligned static Gaxera stack. Limine
/// must have completed its documented x86-64 handoff before reaching `_start`.
#[unsafe(no_mangle)]
#[allow(unreachable_code)] // `panic-test` intentionally terminates before normal boot continues.
pub unsafe extern "C" fn gaxera_rust_entry() -> ! {
    unsafe { serial::COM1.init() }

    // SAFETY: the entry trampoline established the static bootstrap stack
    // before this Rust code executes. Descriptor setup remains single-core and
    // interrupts are disabled by the Limine entry contract.
    unsafe {
        arch::x86_64::cpu::init_bsp_cpu_local();
        arch::x86_64::descriptors::init();
        arch::x86_64::exceptions::init();
        arch::x86_64::syscall::enable_syscalls();
    }
    println!("GAXERA: BOOT_STACK_READY");
    println!("GAXERA: DESCRIPTORS_AND_IDT_READY");

    let handoff = match arch::x86_64::boot::capture_handoff() {
        Ok(handoff) => handoff,
        Err(error) => {
            println!("GAXERA ERROR: Boot handoff capture failed: {error}");
            serial::halt();
        }
    };
    let boot_context = handoff.context();
    println!(
        "GAXERA: BOOT_CONTEXT_READY kernel_phys={:#018x} kernel_virt={:#018x} rsdp_phys={:#018x}",
        boot_context.kernel_image().physical_base,
        boot_context.kernel_image().virtual_base,
        boot_context.rsdp().map_or(0, |rsdp| rsdp.physical_address),
    );
    boot_context.dump_memory_map();

    let mut bootstrap_frames = match BootstrapFrameAllocator::from_boot_context(boot_context) {
        Ok(allocator) => allocator,
        Err(error) => {
            println!("GAXERA ERROR: Bootstrap frame allocator failed: {error}");
            serial::halt();
        }
    };
    println!(
        "GAXERA: BOOTSTRAP_FRAME_ALLOCATOR_READY ranges={}",
        bootstrap_frames.usable_ranges().len()
    );
    let mut page_tables = match unsafe {
        arch::x86_64::paging::KernelPageTables::build(
            boot_context,
            handoff.pre_cr3_hhdm_offset(),
            &mut bootstrap_frames,
        )
    } {
        Ok(page_tables) => page_tables,
        Err(error) => {
            println!("GAXERA ERROR: Page-table construction failed: {error}");
            serial::halt();
        }
    };
    println!(
        "GAXERA: PAGE_TABLES_READY root={:#018x} reservations={}",
        page_tables.root_frame().start_address().as_u64(),
        bootstrap_frames.reservations().ranges().len(),
    );
    if let Err(error) = unsafe { page_tables.activate() } {
        println!("GAXERA ERROR: CR3 activation failed: {error}");
        serial::halt();
    }
    println!("GAXERA: CR3_GAXERA_OWNED");

    let bitmap_words = match SegmentedBitmapFrameAllocator::required_words(boot_context) {
        Ok(words) => words,
        Err(error) => {
            println!("GAXERA ERROR: Bitmap sizing failed: {error}");
            serial::halt();
        }
    };
    let bitmap_bytes = match bitmap_words.checked_mul(mem::size_of::<u64>()) {
        Some(bytes) => bytes,
        None => {
            println!("GAXERA ERROR: Bitmap byte size overflow");
            serial::halt();
        }
    };
    let bitmap_frames = match u64::try_from(bitmap_bytes.div_ceil(PAGE_SIZE as usize)) {
        Ok(frames) => frames,
        Err(_) => {
            println!("GAXERA ERROR: Bitmap frame count overflow");
            serial::halt();
        }
    };
    let bitmap_range = match bootstrap_frames.allocate_contiguous(bitmap_frames) {
        Ok(Some(range)) => range,
        Ok(None) => {
            println!("GAXERA ERROR: Bitmap backing allocation exhausted");
            serial::halt();
        }
        Err(error) => {
            println!("GAXERA ERROR: Bitmap backing allocation failed: {error}");
            serial::halt();
        }
    };
    let bitmap_virtual = match kernel::memory::mapping::HHDM_BASE.checked_add(bitmap_range.start) {
        Some(address) => address,
        None => {
            println!("GAXERA ERROR: Bitmap HHDM address overflow");
            serial::halt();
        }
    };
    // SAFETY: contiguous bootstrap frames were permanently reserved, Gaxera's
    // HHDM maps usable RAM, and this remains the only mutable allocator user.
    let physical_frames = match unsafe {
        initialize_global_allocator(
            boot_context,
            bootstrap_frames.reservations(),
            bitmap_virtual as *mut u64,
            bitmap_words,
        )
    } {
        Ok(allocator) => allocator,
        Err(error) => {
            println!("GAXERA ERROR: Segmented frame allocator failed: {error}");
            serial::halt();
        }
    };
    println!(
        "GAXERA: PHYSICAL_FRAME_ALLOCATOR_READY frames={} bitmap_words={}",
        physical_frames.frame_count(),
        bitmap_words
    );

    let mut heap_offset = 0_u64;
    let mut first_heap_frame = None;
    while heap_offset < HEAP_SIZE {
        let Some(frame) = physical_frames.allocate() else {
            println!("GAXERA ERROR: Heap frame allocation exhausted");
            serial::halt();
        };
        let virtual_address = match HEAP_START.checked_add(heap_offset) {
            Some(address) => address,
            None => {
                println!("GAXERA ERROR: Heap virtual address overflow");
                serial::halt();
            }
        };
        if let Err(error) =
            unsafe { page_tables.map_heap_page(virtual_address, frame, physical_frames) }
        {
            println!("GAXERA ERROR: Heap page mapping failed: {error}");
            serial::halt();
        }
        if heap_offset == 0 {
            first_heap_frame = Some(frame);
        }
        heap_offset += PAGE_SIZE;
    }
    let first_heap_frame = first_heap_frame.expect("non-empty Phase 4 heap");
    // SAFETY: interrupts remain disabled and this bootstrap code is the only
    // page-table mutator, so no concurrent hierarchy mutation can occur.
    if unsafe { page_tables.translate(HEAP_START) } != Some(first_heap_frame.start_address()) {
        println!("GAXERA ERROR: Heap translation self-check failed");
        serial::halt();
    }
    // SAFETY: every heap page was mapped above with writable, NX permissions;
    // both adjacent guard pages remain deliberately absent.
    if let Err(error) =
        unsafe { kernel::memory::heap::init(HEAP_START as usize, HEAP_SIZE as usize) }
    {
        println!("GAXERA ERROR: Heap initialization failed: {error}");
        serial::halt();
    }
    let boxed = Box::new(0x4761_7865_7261_0004_u64);
    let mut values = Vec::with_capacity(64);
    for value in 0_u64..64 {
        values.push(value * value);
    }
    if *boxed != 0x4761_7865_7261_0004 || values.len() != 64 || values[63] != 3969 {
        println!("GAXERA ERROR: Heap allocation self-check failed");
        serial::halt();
    }
    drop(values);
    drop(boxed);
    println!(
        "GAXERA: MEMORY_FOUNDATION_OK heap={:#018x} size={} lower_guard={:#018x}",
        HEAP_START, HEAP_SIZE, HEAP_LOWER_GUARD
    );

    #[cfg(feature = "test-memory")]
    unsafe {
        arch::x86_64::qemu::exit_success();
    }

    #[cfg(feature = "test-heap-guard")]
    unsafe {
        // SAFETY: the lower adjacent page is intentionally unmapped and this
        // volatile load must reach the page-fault handler with CR2 unchanged.
        core::ptr::read_volatile(HEAP_LOWER_GUARD as *const u8);
    }

    #[cfg(feature = "panic-test")]
    panic!("intentional Phase 2 panic proof");

    println!("GAXERA: KERNEL_ENTRY_OK");

    #[cfg(any(
        feature = "test-breakpoint",
        feature = "test-divide-error",
        feature = "test-invalid-opcode",
        feature = "test-general-protection",
        feature = "test-page-fault",
        feature = "test-double-fault",
    ))]
    arch::x86_64::test::run();

    if let Some(info) = boot_context.framebuffer() {
        let framebuffer_virtual = match arch::x86_64::paging::framebuffer_virtual_address(info) {
            Ok(address) => address,
            Err(error) => {
                println!("GAXERA ERROR: Framebuffer mapping lookup failed: {error}");
                serial::halt();
            }
        };
        println!(
            "GAXERA: FRAMEBUFFER_OK ({}x{}, pitch: {})",
            info.width, info.height, info.pitch
        );
        // SAFETY: `BootContext` validated the framebuffer layout and Gaxera's
        // inactive page-table construction mapped this range before CR3 swap.
        match unsafe { framebuffer::Framebuffer::from_boot_context(info, framebuffer_virtual) } {
            Ok(framebuffer) => {
                framebuffer.draw_test_pattern();
                println!("GAXERA: TEST_PATTERN_DRAWN");
            }
            Err(error) => println!("GAXERA ERROR: Unsupported framebuffer: {error}"),
        }
    } else {
        println!("GAXERA ERROR: No framebuffers found in response");
    }

    #[cfg(feature = "test-boot")]
    unsafe {
        arch::x86_64::qemu::exit_success();
    }

    let local_apic_info = match unsafe {
        arch::x86_64::acpi::discover_from_boot_context(
            boot_context,
            &mut page_tables,
            physical_frames,
        )
    } {
        Ok(info) => info,
        Err(error) => {
            println!("GAXERA ERROR: ACPI Local APIC discovery failed: {error}");
            serial::halt();
        }
    };
    println!(
        "GAXERA: ACPI_MADT_READY local_apic_phys={:#018x} override={}",
        local_apic_info.physical_address, local_apic_info.used_address_override
    );
    // SAFETY: interrupts remain disabled and this bootstrap path exclusively
    // owns page-table mutation. ACPI discovery must not retain its fixed
    // temporary mapping once it has copied the parsed table bytes.
    if unsafe { page_tables.translate(kernel::memory::mapping::ACPI_TABLE_WINDOW) }.is_some() {
        println!("GAXERA ERROR: ACPI temporary mapping was not released");
        serial::halt();
    }
    println!("GAXERA: ACPI_TEMPORARY_WINDOW_RELEASED");
    let local_apic = match unsafe {
        arch::x86_64::apic::initialize(
            boot_context,
            local_apic_info,
            &mut page_tables,
            physical_frames,
        )
    } {
        Ok(apic) => apic,
        Err(error) => {
            println!("GAXERA ERROR: Local APIC initialization failed: {error}");
            serial::halt();
        }
    };
    println!(
        "GAXERA: LOCAL_APIC_READY phys={:#018x} vector={:#04x}",
        local_apic.physical_address(),
        arch::x86_64::apic::TIMER_VECTOR
    );

    #[cfg(not(feature = "test-apic-timer"))]
    {
        let cal = match arch::x86_64::apic::calibrate_timer() {
            Ok(c) => c,
            Err(e) => {
                println!("GAXERA ERROR: APIC timer calibration failed: {e}");
                serial::halt();
            }
        };

        let cpu_local = unsafe { arch::x86_64::cpu::get_cpu_local() };
        let timer_queue =
            kernel_core::timer::TimerQueue::try_new(16).expect("TimerQueue allocation failed");
        unsafe {
            *cpu_local.timer_queue.get() = Some(timer_queue);
        }

        if let Err(e) = arch::x86_64::apic::start_preemption_timer(cal, 1) {
            println!("GAXERA ERROR: Failed to start preemption timer: {e}");
            serial::halt();
        }

        println!(
            "GAXERA: PREEMPTION_TIMER_READY ticks_per_ms={}",
            cal.ticks_per_ms
        );
    }

    #[cfg(feature = "test-apic-timer")]
    arch::x86_64::apic::run_timer_delivery_test();

    #[cfg(any(
        feature = "test-user-transition",
        feature = "test-user-privilege",
        feature = "test-user-invalid-frame",
        feature = "test-syscall-round-trip"
    ))]
    {
        let probe = arch::x86_64::probe::M2AProbe::build(&page_tables, physical_frames)
            .expect("GAXERA ERROR: M2A probe construction failed");
        probe.execute();
    }

    #[cfg(any(
        feature = "test-cooperative-yield",
        feature = "test-context-preservation"
    ))]
    {
        arch::x86_64::test_yield::run_cooperative_yield_test(&mut page_tables, physical_frames);
    }

    #[cfg(feature = "test-user-copy-fault")]
    {
        let mut buf = [0u8; 16];
        let res = arch::x86_64::user_copy::copy_from_user(&mut buf, 0x0, 16);
        if res == Err(arch::x86_64::user_copy::UserCopyError::Fault)
            || res == Err(arch::x86_64::user_copy::UserCopyError::InvalidPointer)
        {
            println!("GAXERA: USER_COPY_FAULT_RECOVERED_OK");
            #[cfg(feature = "qemu-test")]
            unsafe {
                arch::x86_64::qemu::exit_success()
            };
        }
        println!("GAXERA ERROR: USER_COPY_FAULT_NOT_RECOVERED");
        #[cfg(feature = "qemu-test")]
        unsafe {
            arch::x86_64::qemu::exit_failure()
        };
    }

    #[cfg(feature = "test-ipc")]
    {
        arch::x86_64::test_ipc::run_ipc_test();
    }

    #[cfg(feature = "test-preemption")]
    {
        arch::x86_64::test_preemption::run_preemption_test(&mut page_tables, physical_frames);
    }

    #[cfg(not(any(
        feature = "test-apic-timer",
        feature = "test-user-transition",
        feature = "test-user-privilege",
        feature = "test-user-invalid-frame",
        feature = "test-syscall-round-trip",
        feature = "test-user-copy-fault",
        feature = "test-cooperative-yield",
        feature = "test-context-preservation",
        feature = "test-ipc",
        feature = "test-preemption"
    )))]
    {
        x86_64::instructions::interrupts::enable();
        serial::idle();
    }
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    println!(
        "GAXERA ERROR: HEAP_ALLOCATION_FAILED size={} align={}",
        layout.size(),
        layout.align()
    );
    serial::halt();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!(
            "GAXERA KERNEL PANIC at {}:{}:{}: {}",
            location.file(),
            location.line(),
            location.column(),
            info.message()
        );
    } else {
        println!("GAXERA KERNEL PANIC: {info}");
    }

    arch::x86_64::diagnostics::emit_panic_telemetry();

    #[cfg(feature = "panic-test")]
    unsafe {
        arch::x86_64::qemu::exit_success();
    }

    #[cfg(not(feature = "panic-test"))]
    serial::halt();
}
