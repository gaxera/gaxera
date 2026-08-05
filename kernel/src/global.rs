use core::ops::{Deref, DerefMut};
#[cfg(test)]
use core::sync::atomic::Ordering;
use kernel_core::address_space::AddressSpace;
use kernel_core::capability::{CapabilitySpace, CapabilitySystem};
use kernel_core::debug_console::DebugConsole;
use kernel_core::ipc::Endpoint;
use kernel_core::memory::MemoryObject;
use kernel_core::object::{ObjectArena, ResourceDomain};
use kernel_core::registry::BTreeRegistry;
use spinning_top::Spinlock;

// NOTE(SMP-DEFERRED): Lock rank enforcement is currently test-only because the BSP
// kernel runs single-CPU with interrupts disabled during syscall dispatch. For the
// future SMP port (ADR 0031), this must become a per-CPU runtime check.
#[cfg(test)]
std::thread_local! {
    static CURRENT_LOCK_RANK: core::sync::atomic::AtomicU8 = const { core::sync::atomic::AtomicU8::new(255) };
}

/// A wrapper around `Spinlock<T>` that enforces rank-ordered lock acquisition (ADR 0033).
///
/// In test mode, acquiring a `RankedLock` evaluates held lock rank to prevent out-of-order deadlocks.
pub struct RankedLock<T, const LEVEL: u8> {
    inner: Spinlock<T>,
}

impl<T, const LEVEL: u8> RankedLock<T, LEVEL> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: Spinlock::new(value),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }

    pub fn lock(&self) -> RankedLockGuard<'_, T, LEVEL> {
        #[cfg(test)]
        let prev_rank = CURRENT_LOCK_RANK.with(|rank| {
            let current = rank.load(Ordering::Acquire);
            if current != 255 {
                if LEVEL == 4 && current == 4 {
                    panic!(
                        "LOCK HIERARCHY VIOLATION: Parallel Level 4 registry locks cannot be nested!"
                    );
                } else if LEVEL <= current {
                    panic!(
                        "LOCK HIERARCHY VIOLATION: Attempted to acquire Level {} lock while holding Level {} lock!",
                        LEVEL, current
                    );
                }
            }
            rank.swap(LEVEL, Ordering::AcqRel)
        });

        let guard = self.inner.lock();

        RankedLockGuard {
            guard,
            #[cfg(test)]
            _prev_rank: prev_rank,
        }
    }

    /// Attempt a non-blocking acquisition.  Interrupt-context code may use
    /// this only as an opportunistic fast path; callers must provide an
    /// allocation-free fallback when it returns `None`.
    pub fn try_lock(&self) -> Option<RankedLockGuard<'_, T, LEVEL>> {
        let guard = self.inner.try_lock()?;
        Some(RankedLockGuard {
            guard,
            #[cfg(test)]
            _prev_rank: 255,
        })
    }
}

pub struct RankedLockGuard<'a, T, const LEVEL: u8> {
    guard: spinning_top::guard::SpinlockGuard<'a, T>,
    #[cfg(test)]
    _prev_rank: u8,
}

impl<'a, T, const LEVEL: u8> Deref for RankedLockGuard<'a, T, LEVEL> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T, const LEVEL: u8> DerefMut for RankedLockGuard<'a, T, LEVEL> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

#[cfg(test)]
impl<'a, T, const LEVEL: u8> Drop for RankedLockGuard<'a, T, LEVEL> {
    fn drop(&mut self) {
        CURRENT_LOCK_RANK.with(|rank| {
            rank.store(self._prev_rank, Ordering::Release);
        });
    }
}

/// TOTAL GLOBAL LOCK ORDERING CONTRACT (ADR 0033):
/// Level 0: RESOURCE_DOMAINS (Resource quota management)
/// Level 1: CAPABILITY_SYSTEM (Global capability lineage and derivation tree)
/// Level 2: OBJECT_ARENA (Object slot and generation tracker)
/// Level 3: PHYSICAL_ALLOCATOR (Physical frame allocator)
/// Level 4: Typed Object Registries (ENDPOINTS, ADDRESS_SPACES, CAPABILITY_SPACES,
///          MEMORY_OBJECTS, DEBUG_CONSOLES, FACTORIES, WAIT_SETS, NOTIFICATIONS,
///          INTERRUPTS, MAPPINGS, CONTIGUOUS_FRAMES, PROCESSES)
pub static RESOURCE_DOMAINS: RankedLock<BTreeRegistry<ResourceDomain>, 0> =
    RankedLock::new(BTreeRegistry::new());
pub static CAPABILITY_SYSTEM: RankedLock<Option<CapabilitySystem>, 1> = RankedLock::new(None);
pub static OBJECT_ARENA: RankedLock<Option<ObjectArena>, 2> = RankedLock::new(None);
pub static PHYSICAL_ALLOCATOR: RankedLock<
    Option<&'static mut crate::memory::physical::SegmentedBitmapFrameAllocator<'static>>,
    3,
> = RankedLock::new(None);

pub static ENDPOINTS: RankedLock<BTreeRegistry<Endpoint>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static ADDRESS_SPACES: RankedLock<
    BTreeRegistry<AddressSpace<crate::arch::x86_64::address_space::X86AddressSpace>>,
    4,
> = RankedLock::new(BTreeRegistry::new());
pub static CAPABILITY_SPACES: RankedLock<BTreeRegistry<CapabilitySpace>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static MEMORY_OBJECTS: RankedLock<BTreeRegistry<MemoryObject>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static DEBUG_CONSOLES: RankedLock<BTreeRegistry<DebugConsole>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static FACTORIES: RankedLock<BTreeRegistry<kernel_core::object::Factory>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static WAIT_SETS: RankedLock<BTreeRegistry<kernel_core::waitset::WaitSet>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static NOTIFICATIONS: RankedLock<BTreeRegistry<kernel_core::notification::Notification>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static INTERRUPTS: RankedLock<BTreeRegistry<kernel_core::interrupt::InterruptObject>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static MAPPINGS: RankedLock<BTreeRegistry<kernel_core::mapping::Mapping>, 4> =
    RankedLock::new(BTreeRegistry::new());
pub static CONTIGUOUS_FRAMES: RankedLock<
    BTreeRegistry<kernel_core::contiguous_frame::ContiguousFrameObject>,
    4,
> = RankedLock::new(BTreeRegistry::new());
pub static PROCESSES: RankedLock<BTreeRegistry<kernel_core::process::Process>, 4> =
    RankedLock::new(BTreeRegistry::new());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_lock_ordering_sequence() {
        let lock0 = RankedLock::<u32, 0>::new(0);
        let lock1 = RankedLock::<u32, 1>::new(1);
        let lock2 = RankedLock::<u32, 2>::new(2);
        let lock4 = RankedLock::<u32, 4>::new(4);

        let g0 = lock0.lock();
        let g1 = lock1.lock();
        let g2 = lock2.lock();
        let g4 = lock4.lock();

        assert_eq!(*g0, 0);
        assert_eq!(*g1, 1);
        assert_eq!(*g2, 2);
        assert_eq!(*g4, 4);
    }

    #[test]
    #[should_panic(expected = "LOCK HIERARCHY VIOLATION")]
    fn test_invalid_lock_ordering_inversion_panics() {
        let lock1 = RankedLock::<u32, 1>::new(1);
        let lock4 = RankedLock::<u32, 4>::new(4);

        let _g4 = lock4.lock();
        let _g1 = lock1.lock(); // Should panic due to out-of-order acquisition
    }

    #[test]
    #[should_panic(expected = "LOCK HIERARCHY VIOLATION")]
    fn test_parallel_level_4_nesting_panics() {
        let lock4a = RankedLock::<u32, 4>::new(40);
        let lock4b = RankedLock::<u32, 4>::new(41);

        let _g4a = lock4a.lock();
        let _g4b = lock4b.lock(); // Should panic due to parallel registry nesting
    }
}
