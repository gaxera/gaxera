use crate::{Handle, Rights};

pub const INLINE_MESSAGE_BYTES: usize = 64;
pub const MAX_CAPABILITY_TRANSFERS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum IpcStatus {
    Success = 0,
    InvalidHandle = 1,
    DeniedRights = 2,
    InvalidEndpoint = 3,
    MessageTooLarge = 4,
    TransferLimitExceeded = 5,
    EndpointClosed = 6,
    StaleReply = 7,
    CapacityExhausted = 8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferDescriptor {
    pub handle: Handle,
    pub narrowed_rights: Rights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineMessage {
    bytes: [u8; INLINE_MESSAGE_BYTES],
    length: usize,
}

impl InlineMessage {
    pub fn try_new(payload: &[u8]) -> Result<Self, IpcStatus> {
        if payload.len() > INLINE_MESSAGE_BYTES {
            return Err(IpcStatus::MessageTooLarge);
        }
        let mut bytes = [0u8; INLINE_MESSAGE_BYTES];
        let length = payload.len();
        if length > 0 {
            bytes[..length].copy_from_slice(payload);
        }
        Ok(Self { bytes, length })
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

/// A one-use reply authority token.
/// Represents endpoint generation, sequence, and caller identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ReplyToken(u64);

impl ReplyToken {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_message_zero_length() {
        let msg = InlineMessage::try_new(&[]).unwrap();
        assert_eq!(msg.payload().len(), 0);
    }

    #[test]
    fn inline_message_full_length() {
        let payload = [0xAA; INLINE_MESSAGE_BYTES];
        let msg = InlineMessage::try_new(&payload).unwrap();
        assert_eq!(msg.payload().len(), INLINE_MESSAGE_BYTES);
        assert_eq!(msg.payload(), &payload[..]);
    }

    #[test]
    fn inline_message_oversized_rejection() {
        let payload = [0xAA; INLINE_MESSAGE_BYTES + 1];
        assert_eq!(
            InlineMessage::try_new(&payload).map(|_| ()),
            Err(IpcStatus::MessageTooLarge)
        );
    }

    #[test]
    fn reply_token_encoding() {
        let raw = 0xDEADBEEFCAFE0001;
        let token = ReplyToken::from_raw(raw);
        assert_eq!(token.raw(), raw);
    }

    #[test]
    fn transfer_descriptor_representation() {
        let handle = Handle::from_parts(1, 1);
        let rights = Rights::READ | Rights::WRITE;
        let desc = TransferDescriptor {
            handle,
            narrowed_rights: rights,
        };
        assert_eq!(desc.handle.raw(), handle.raw());
        assert_eq!(desc.narrowed_rights.bits(), rights.bits());
    }
}
