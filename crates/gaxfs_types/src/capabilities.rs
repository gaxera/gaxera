//! GaxFS Capability Model & Rights Definitions
//!
//! Enforces Zero Ambient Authority security and capability derivation monotonicity.

use crate::object_id::GaxObjectId;
use core::fmt;

/// Extensible bitfield rights flags attached to a capability handle.
pub struct GaxFsRights;

impl GaxFsRights {
    pub const NONE: u32 = 0;
    pub const READ: u32 = 1 << 0; // Read object payload extents
    pub const WRITE: u32 = 1 << 1; // Overwrite payload extents
    pub const APPEND: u32 = 1 << 2; // Append payload data
    pub const EXECUTE: u32 = 1 << 3; // Execute binary payload
    pub const ENUMERATE: u32 = 1 << 4; // List namespace provider entries
    pub const SNAPSHOT: u32 = 1 << 5; // Create point-in-time snapshot
    pub const SHARE: u32 = 1 << 6; // Derive & delegate capability handles
    pub const DELETE: u32 = 1 << 7; // Unlink / logically delete object
    pub const MODIFY_METADATA: u32 = 1 << 8; // Edit attributes & graph relationships
    pub const CREATE_CHILDREN: u32 = 1 << 9; // Create child entries in namespace

    pub const READ_WRITE: u32 = Self::READ | Self::WRITE;
    pub const ALL: u32 = 0xFFFFFFFF;
}

/// Identifies an isolated capability delegation space
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CapabilitySpace(pub u64);

/// Capability Handle Authorization Errors
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    AuthorityAmplificationForbidden,
    CapabilityRevoked,
    InsufficientRights { required: u32, granted: u32 },
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorityAmplificationForbidden => {
                write!(
                    f,
                    "Capability Error: Derived rights cannot exceed parent rights"
                )
            }
            Self::CapabilityRevoked => write!(f, "Capability Error: Capability has been revoked"),
            Self::InsufficientRights { required, granted } => write!(
                f,
                "Capability Error: Required rights {:#x}, but handle has {:#x}",
                required, granted
            ),
        }
    }
}

/// Canonical unforgeable capability handle.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CapabilityHandle {
    handle_id: u64,
    space: CapabilitySpace,
    target_object: GaxObjectId,
    rights: u32,
    revoked: bool,
}

impl CapabilityHandle {
    /// Creates a new root capability handle
    pub fn new_root(
        handle_id: u64,
        space: CapabilitySpace,
        target_object: GaxObjectId,
        rights: u32,
    ) -> Self {
        Self {
            handle_id,
            space,
            target_object,
            rights,
            revoked: false,
        }
    }

    /// Handle ID
    pub const fn handle_id(&self) -> u64 {
        self.handle_id
    }

    /// Target object ID
    pub const fn target_object(&self) -> GaxObjectId {
        self.target_object
    }

    /// Granted rights bitmask
    pub const fn rights(&self) -> u32 {
        self.rights
    }

    /// Associated capability space
    pub const fn space(&self) -> CapabilitySpace {
        self.space
    }

    /// Returns true if capability is revoked
    pub const fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Verifies if handle contains required rights flags
    pub fn check_rights(&self, required_rights: u32) -> Result<(), CapabilityError> {
        if self.revoked {
            return Err(CapabilityError::CapabilityRevoked);
        }
        if (self.rights & required_rights) != required_rights {
            return Err(CapabilityError::InsufficientRights {
                required: required_rights,
                granted: self.rights,
            });
        }
        Ok(())
    }

    /// Derives a narrowed capability handle (Invariant 1: Monotonicity).
    /// Authority amplification is strictly forbidden.
    pub fn derive_narrowed(
        &self,
        new_handle_id: u64,
        requested_rights: u32,
    ) -> Result<Self, CapabilityError> {
        if self.revoked {
            return Err(CapabilityError::CapabilityRevoked);
        }

        // Monotonicity Invariant check: requested_rights must be subset of self.rights
        if (requested_rights & !self.rights) != 0 {
            return Err(CapabilityError::AuthorityAmplificationForbidden);
        }

        Ok(Self {
            handle_id: new_handle_id,
            space: self.space,
            target_object: self.target_object,
            rights: requested_rights,
            revoked: false,
        })
    }

    /// Marks handle as revoked
    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rights_attenuation_and_amplification_prevention() {
        let obj_id = GaxObjectId::new_v7(100, 1, 1);
        let parent_handle = CapabilityHandle::new_root(
            1,
            CapabilitySpace(1),
            obj_id,
            GaxFsRights::READ | GaxFsRights::SHARE,
        );

        // Valid attenuation
        let child_handle = parent_handle
            .derive_narrowed(2, GaxFsRights::READ)
            .expect("Derived read handle must succeed");
        assert_eq!(child_handle.rights(), GaxFsRights::READ);

        // Invalid amplification attempt (adding WRITE right when parent doesn't have it)
        let amp_result = parent_handle.derive_narrowed(3, GaxFsRights::READ | GaxFsRights::WRITE);
        assert_eq!(
            amp_result,
            Err(CapabilityError::AuthorityAmplificationForbidden),
            "Authority amplification must be rejected"
        );
    }

    #[test]
    fn test_revocation_behavior() {
        let obj_id = GaxObjectId::new_v7(100, 1, 1);
        let mut handle =
            CapabilityHandle::new_root(1, CapabilitySpace(1), obj_id, GaxFsRights::READ);

        assert!(handle.check_rights(GaxFsRights::READ).is_ok());

        handle.revoke();
        assert_eq!(
            handle.check_rights(GaxFsRights::READ),
            Err(CapabilityError::CapabilityRevoked)
        );
    }
}
