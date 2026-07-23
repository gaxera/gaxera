use alloc::vec::Vec;
use kernel_core::address_space::AddressSpace;
use kernel_core::capability::{CapabilitySpace, CapabilitySystem};
use kernel_core::debug_console::DebugConsole;
use kernel_core::ipc::Endpoint;
use kernel_core::memory::MemoryObject;
use kernel_core::object::{ObjectArena, ResourceDomain};
use kernel_core::registry::BTreeRegistry;
use spinning_top::Spinlock;

/// Level 1 Lock: The global capability derivation tree.
/// Must be locked before OBJECT_ARENA if both are needed.
pub static CAPABILITY_SYSTEM: Spinlock<Option<CapabilitySystem>> = Spinlock::new(None);

/// Level 2 Lock: The global object identity and generation tracker.
/// Never nests other locks.
pub static OBJECT_ARENA: Spinlock<Option<ObjectArena>> = Spinlock::new(None);

/// Level 3 Locks: Typed Object Registries.
/// These are mutually exclusive parallel locks. A registry lock must *never*
/// attempt to acquire another registry lock or higher-level lock.
pub static ENDPOINTS: Spinlock<BTreeRegistry<Endpoint>> = Spinlock::new(BTreeRegistry::new());
pub static ADDRESS_SPACES: Spinlock<
    BTreeRegistry<AddressSpace<crate::arch::x86_64::address_space::X86AddressSpace>>,
> = Spinlock::new(BTreeRegistry::new());
pub static CAPABILITY_SPACES: Spinlock<BTreeRegistry<CapabilitySpace>> =
    Spinlock::new(BTreeRegistry::new());

/// Global Physical Allocator
pub static PHYSICAL_ALLOCATOR: Spinlock<
    Option<&'static mut crate::memory::physical::SegmentedBitmapFrameAllocator<'static>>,
> = Spinlock::new(None);
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

// Note: `THREADS` registry is currently maintained in `arch::x86_64::thread::THREADS`
// due to specialized context-switching borrowing requirements.

/// Global active domain tracker (prevents domains from dropping).
pub static RESOURCE_DOMAINS: Spinlock<Vec<ResourceDomain>> = Spinlock::new(Vec::new());

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
