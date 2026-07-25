/// CPU Affinity Mask abstraction (`CpuAffinityMask`).
///
/// Encapsulates CPU affinity bitmask state for thread scheduling and load balancing.
/// Hides internal bitmask representation to allow future scaling beyond 64 CPUs without changing public APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuAffinityMask {
    mask: u64,
}

impl CpuAffinityMask {
    /// Create an affinity mask permitting execution on all CPUs (0..64).
    pub const fn all() -> Self {
        Self { mask: u64::MAX }
    }

    /// Create an affinity mask pinned to a single CPU core.
    pub const fn single(cpu_id: u32) -> Self {
        if cpu_id >= 64 {
            Self { mask: 0 }
        } else {
            Self {
                mask: 1u64 << cpu_id,
            }
        }
    }

    /// Create a custom affinity mask from a raw `u64` bitmask.
    pub const fn from_raw(mask: u64) -> Self {
        Self { mask }
    }

    /// Return the raw underlying `u64` bitmask.
    pub const fn as_raw(&self) -> u64 {
        self.mask
    }

    /// Returns `true` if the affinity mask permits execution on `cpu_id`.
    pub const fn contains(&self, cpu_id: u32) -> bool {
        if cpu_id >= 64 {
            false
        } else {
            (self.mask & (1u64 << cpu_id)) != 0
        }
    }

    /// Enables or disables execution permission for `cpu_id`.
    pub fn set(&mut self, cpu_id: u32, enabled: bool) {
        if cpu_id < 64 {
            if enabled {
                self.mask |= 1u64 << cpu_id;
            } else {
                self.mask &= !(1u64 << cpu_id);
            }
        }
    }

    /// Returns `true` if the affinity mask permits no CPUs.
    pub const fn is_empty(&self) -> bool {
        self.mask == 0
    }
}

impl Default for CpuAffinityMask {
    fn default() -> Self {
        Self::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_affinity_mask_operations() {
        let all = CpuAffinityMask::all();
        assert!(all.contains(0));
        assert!(all.contains(63));

        let cpu2 = CpuAffinityMask::single(2);
        assert!(!cpu2.contains(0));
        assert!(cpu2.contains(2));
        assert!(!cpu2.contains(3));

        let mut custom = CpuAffinityMask::from_raw(0);
        assert!(custom.is_empty());
        custom.set(4, true);
        assert!(custom.contains(4));
        custom.set(4, false);
        assert!(custom.is_empty());
    }
}
