#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

pub mod arch;
pub mod framebuffer;
pub mod serial;

#[cfg(any(
    all(feature = "panic-test", feature = "test-boot"),
    all(
        feature = "panic-test",
        any(
            feature = "test-breakpoint",
            feature = "test-divide-error",
            feature = "test-invalid-opcode",
            feature = "test-general-protection",
            feature = "test-page-fault",
            feature = "test-double-fault",
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
        )
    ),
))]
compile_error!("exactly one Gaxera QEMU test profile may be enabled");

use core::panic::PanicInfo;
use limine::BaseRevision;
use limine::request::{
    EntryPointRequest, FramebufferRequest, HhdmRequest, MemmapRequest, RsdpRequest,
};

// Request the newest protocol revision supported by the pinned Rust bindings.
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static ENTRY_POINT: EntryPointRequest = EntryPointRequest::new(crate::_start);

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

// These handoff records are deliberately declared now, even though Phase 2
// consumes only the framebuffer. Phases 4 and 5 depend on this same contract.
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

// SAFETY: _start is the sole kernel entry point; no other global symbol in
// this binary can collide with it.
/// # Safety
/// Limine must have completed its documented x86-64 handoff before calling
/// this function. In particular, the initial stack and request responses must
/// still be mapped by Limine's page tables.
#[unsafe(no_mangle)]
#[allow(unreachable_code)]
pub unsafe extern "C" fn _start() -> ! {
    // SAFETY: Limine hands execution to the kernel with interrupts disabled,
    // and QEMU exposes a 16550-compatible UART at COM1 (0x3F8).
    unsafe {
        serial::COM1.init();
    }

    if !BASE_REVISION.is_supported() {
        println!("GAXERA ERROR: Unsupported Limine base revision");
        serial::halt();
    }

    // SAFETY: Limine enters with a valid temporary GDT and stack, interrupts
    // disabled, and no concurrent CPU work. Phase 3 replaces that temporary
    // descriptor state exactly once before loading the IDT.
    unsafe {
        arch::x86_64::descriptors::init();
        arch::x86_64::exceptions::init();
    }
    println!("GAXERA: DESCRIPTORS_AND_IDT_READY");

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

    if let Some(response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(&fb) = response.framebuffers().first() {
            println!(
                "GAXERA: FRAMEBUFFER_OK ({}x{}, {}bpp, pitch: {})",
                fb.width, fb.height, fb.bpp, fb.pitch
            );

            // SAFETY: Base revision 6 guarantees that Limine maps the returned
            // framebuffer for the life of this bootloader-owned address space.
            match unsafe { framebuffer::Framebuffer::from_limine(fb) } {
                Ok(framebuffer) => {
                    framebuffer.draw_test_pattern();
                    println!("GAXERA: TEST_PATTERN_DRAWN");
                }
                Err(error) => println!("GAXERA ERROR: Unsupported framebuffer: {error}"),
            }
        } else {
            println!("GAXERA ERROR: No framebuffers found in response");
        }
    } else {
        println!("GAXERA ERROR: Framebuffer request failed");
    }

    #[cfg(feature = "test-boot")]
    // SAFETY: test-boot is only launched by xtask with isa-debug-exit attached.
    unsafe {
        arch::x86_64::qemu::exit_success();
    }

    #[cfg(not(feature = "test-boot"))]
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

    #[cfg(feature = "panic-test")]
    // SAFETY: panic-test is only launched by xtask with isa-debug-exit attached.
    unsafe {
        arch::x86_64::qemu::exit_success();
    }

    #[cfg(not(feature = "panic-test"))]
    serial::halt();
}
