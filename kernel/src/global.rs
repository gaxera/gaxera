use alloc::vec::Vec;
use kernel_core::address_space::AddressSpace;
use kernel_core::capability::{CapabilitySpace, CapabilitySystem};
use kernel_core::debug_console::DebugConsole;
use kernel_core::ipc::Endpoint;
use kernel_core::memory::MemoryObject;
use kernel_core::object::{ObjectArena, ResourceDomain};
use kernel_core::registry::BTreeRegistry;
use spinning_top::Spinlock;

/// TOTAL GLOBAL LOCK ORDERING CONTRACT:
/// Level 0: RESOURCE_DOMAINS (Resource quota management)
/// Level 1: CAPABILITY_SYSTEM (Global capability lineage and derivation tree)
/// Level 2: OBJECT_ARENA (Object slot and generation tracker)
/// Level 3: PHYSICAL_ALLOCATOR (Physical frame allocator)
/// Level 4: Typed Object Registries (ENDPOINTS, ADDRESS_SPACES, CAPABILITY_SPACES,
///          MEMORY_OBJECTS, DEBUG_CONSOLES, FACTORIES, WAIT_SETS, NOTIFICATIONS,
///          INTERRUPTS, MAPPINGS)
///
/// Invariants:
/// 1. Locks MUST be acquired strictly in increasing Level order (Level 0 -> 1 -> 2 -> 3 -> 4).
/// 2. Parallel Level 4 registry locks MUST NEVER be nested together.
/// 3. Global locks MUST NEVER be held across user copies, thread scheduling, context switches, or device I/O.
pub static RESOURCE_DOMAINS: Spinlock<Vec<ResourceDomain>> = Spinlock::new(Vec::new());
pub static CAPABILITY_SYSTEM: Spinlock<Option<CapabilitySystem>> = Spinlock::new(None);
pub static OBJECT_ARENA: Spinlock<Option<ObjectArena>> = Spinlock::new(None);
pub static PHYSICAL_ALLOCATOR: Spinlock<
    Option<&'static mut crate::memory::physical::SegmentedBitmapFrameAllocator<'static>>,
> = Spinlock::new(None);

pub static ENDPOINTS: Spinlock<BTreeRegistry<Endpoint>> = Spinlock::new(BTreeRegistry::new());
pub static ADDRESS_SPACES: Spinlock<
    BTreeRegistry<AddressSpace<crate::arch::x86_64::address_space::X86AddressSpace>>,
> = Spinlock::new(BTreeRegistry::new());
pub static CAPABILITY_SPACES: Spinlock<BTreeRegistry<CapabilitySpace>> =
    Spinlock::new(BTreeRegistry::new());
pub static MEMORY_OBJECTS: Spinlock<BTreeRegistry<MemoryObject>> =
    Spinlock::new(BTreeRegistry::new());
pub static DEBUG_CONSOLES: Spinlock<BTreeRegistry<DebugConsole>> =
    Spinlock::new(BTreeRegistry::new());
pub static FACTORIES: Spinlock<BTreeRegistry<kernel_core::object::Factory>> =
    Spinlock::new(BTreeRegistry::new());
pub static WAIT_SETS: Spinlock<BTreeRegistry<kernel_core::waitset::WaitSet>> =
    Spinlock::new(BTreeRegistry::new());
pub static NOTIFICATIONS: Spinlock<BTreeRegistry<kernel_core::notification::Notification>> =
    Spinlock::new(BTreeRegistry::new());
pub static INTERRUPTS: Spinlock<BTreeRegistry<kernel_core::interrupt::InterruptObject>> =
    Spinlock::new(BTreeRegistry::new());
pub static MAPPINGS: Spinlock<BTreeRegistry<kernel_core::mapping::Mapping>> =
    Spinlock::new(BTreeRegistry::new());
pub static CONTIGUOUS_FRAMES: Spinlock<
    BTreeRegistry<kernel_core::contiguous_frame::ContiguousFrameObject>,
> = Spinlock::new(BTreeRegistry::new());

// Note: `THREADS` registry is currently maintained in `arch::x86_64::thread::THREADS`
// due to specialized context-switching borrowing requirements.

/// Initializes the global kernel state.
///
/// # Panics
/// Panics if the state is already initialized.
pub fn init(arena: ObjectArena, system: CapabilitySystem) {
    let mut arena_guard = OBJECT_ARENA.lock();
    if arena_guard.is_some() {
        panic!("GlobalState (OBJECT_ARENA) already initialized");
    }
    *arena_guard = Some(arena);

    let mut sys_guard = CAPABILITY_SYSTEM.lock();
    if sys_guard.is_some() {
        panic!("GlobalState (CAPABILITY_SYSTEM) already initialized");
    }
    *sys_guard = Some(system);
}
