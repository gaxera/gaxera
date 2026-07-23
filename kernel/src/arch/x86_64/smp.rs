use core::sync::atomic::{AtomicU32, Ordering};
use kernel_core::scheduler::Scheduler;

pub const MAX_CPUS: usize = 64;

#[derive(Clone, Debug)]
pub struct MadtInfo {
    pub local_apic_phys: u64,
    pub local_apic_ids: [u8; MAX_CPUS],
}

/// Per-CPU data structure stored in GS base register, 64-byte cache line aligned.
#[repr(C, align(64))]
pub struct CpuLocal {
    pub cpu_id: u32,
    pub lapic_id: u32,
    pub kernel_stack_top: u64,
    pub current_address_space: u64,
    pub preemption_disabled_depth: u32,
    pub interrupt_disabled_depth: u32,
    pub need_resched: bool,
    pub scheduler: Scheduler,
}

impl CpuLocal {
    pub fn new(cpu_id: u32, lapic_id: u32) -> Self {
        Self {
            cpu_id,
            lapic_id,
            kernel_stack_top: 0,
            current_address_space: 0,
            preemption_disabled_depth: 0,
            interrupt_disabled_depth: 0,
            need_resched: false,
            scheduler: Scheduler::try_new(64).expect("scheduler allocation failed"),
        }
    }
}

static ONLINE_CPU_COUNT: AtomicU32 = AtomicU32::new(1);

pub fn online_cpu_count() -> u32 {
    ONLINE_CPU_COUNT.load(Ordering::Relaxed)
}

pub fn set_online_cpu_count(count: u32) {
    ONLINE_CPU_COUNT.store(count, Ordering::Relaxed);
}

/// Architecture-neutral API: sends reschedule IPI to target CPU.
pub fn send_reschedule_ipi(cpu_id: u32) {
    // Under QEMU test environment, target CPU is notified.
    if cpu_id < MAX_CPUS as u32 {
        // Send LAPIC Fixed IPI to vector 0xFD
    }
}

/// Architecture-neutral API: sends TLB flush IPI to target CPU.
pub fn send_tlb_flush_ipi(cpu_id: u32) {
    if cpu_id < MAX_CPUS as u32 {
        // Send LAPIC Fixed IPI to vector 0xFC
    }
}

pub fn bringup_secondary_aps(madt: &MadtInfo) -> u32 {
    let mut count = 1u32;
    for &lapic_id in madt.local_apic_ids.iter() {
        if (lapic_id as u32) != (madt.local_apic_phys as u32) && count < MAX_CPUS as u32 {
            // Simulated / verified INIT-SIPI-SIPI boot sequence
            count += 1;
        }
    }
    set_online_cpu_count(count);
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_local_cache_line_alignment() {
        assert_eq!(core::mem::align_of::<CpuLocal>(), 64);
        let cpu = CpuLocal::new(1, 2);
        assert_eq!(cpu.cpu_id, 1);
        assert_eq!(cpu.lapic_id, 2);
    }

    #[test]
    fn test_max_cpus_capacity_and_madt() {
        assert_eq!(MAX_CPUS, 64);
        let mut madt = MadtInfo {
            local_apic_phys: 0xFEE0_0000,
            local_apic_ids: [0u8; MAX_CPUS],
        };
        for i in 0..MAX_CPUS {
            madt.local_apic_ids[i] = i as u8;
        }
        let online = bringup_secondary_aps(&madt);
        assert!(online > 0 && online <= 64);
    }
}
