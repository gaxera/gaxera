use crate::capability::{CapabilityError, CapabilitySpace, CapabilitySystem, PreparedTransfer};
use crate::object::{ObjectArena, ObjectId};
use crate::resource::ResourceDomain;
use alloc::vec::Vec;
use gaxera_abi::Handle;
use gaxera_abi::ipc::{InlineMessage, MAX_CAPABILITY_TRANSFERS, ReplyToken, TransferDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EndpointError {
    Busy,
    Closed,
    InvalidReplyToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IpcEffect {
    Block,
    Wake(ObjectId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedCall {
    pub caller: ObjectId,
    pub message: InlineMessage,
    pub reply_token: ReplyToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointCloseEffects {
    pub woke_threads: Vec<ObjectId>,
}

#[derive(Clone, Debug)]
enum EndpointState {
    Idle,
    CallerPending {
        caller: ObjectId,
        message: InlineMessage,
    },
    ReceiverWaiting {
        receiver: ObjectId,
    },
    ReplyOutstanding {
        caller: ObjectId,
        token: ReplyToken,
        // The message is stored here if delivered to a waiting receiver,
        // until the receiver explicitly takes it upon waking.
        message: Option<InlineMessage>,
    },
}

pub struct Endpoint {
    state: EndpointState,
    reply_sequence: u64,
    closed: bool,
}

impl Endpoint {
    pub fn new(_id: ObjectId) -> Self {
        Self {
            state: EndpointState::Idle,
            reply_sequence: 0,
            closed: false,
        }
    }

    pub fn call(
        &mut self,
        caller: ObjectId,
        message: InlineMessage,
    ) -> Result<IpcEffect, EndpointError> {
        if self.closed {
            return Err(EndpointError::Closed);
        }

        match self.state {
            EndpointState::Idle => {
                self.state = EndpointState::CallerPending { caller, message };
                Ok(IpcEffect::Block)
            }
            EndpointState::ReceiverWaiting { receiver } => {
                self.reply_sequence += 1;
                let token = ReplyToken::from_raw(self.reply_sequence);
                self.state = EndpointState::ReplyOutstanding {
                    caller,
                    token,
                    message: Some(message),
                };
                Ok(IpcEffect::Wake(receiver))
            }
            EndpointState::CallerPending { .. } | EndpointState::ReplyOutstanding { .. } => {
                Err(EndpointError::Busy)
            }
        }
    }

    pub fn receive(
        &mut self,
        receiver: ObjectId,
    ) -> Result<Result<ReceivedCall, IpcEffect>, EndpointError> {
        if self.closed {
            return Err(EndpointError::Closed);
        }

        match self.state {
            EndpointState::Idle => {
                self.state = EndpointState::ReceiverWaiting { receiver };
                Ok(Err(IpcEffect::Block))
            }
            EndpointState::CallerPending { caller, message } => {
                self.reply_sequence += 1;
                let token = ReplyToken::from_raw(self.reply_sequence);
                self.state = EndpointState::ReplyOutstanding {
                    caller,
                    token,
                    message: None, // Delivered immediately, no need to store
                };
                Ok(Ok(ReceivedCall {
                    caller,
                    message,
                    reply_token: token,
                }))
            }
            EndpointState::ReceiverWaiting { .. } | EndpointState::ReplyOutstanding { .. } => {
                Err(EndpointError::Busy)
            }
        }
    }

    pub fn take_received_call(&mut self) -> Option<ReceivedCall> {
        match &mut self.state {
            EndpointState::ReplyOutstanding {
                caller,
                token,
                message,
            } => message.take().map(|msg| ReceivedCall {
                caller: *caller,
                message: msg,
                reply_token: *token,
            }),
            _ => None,
        }
    }

    pub fn reply(
        &mut self,
        token: ReplyToken,
        _message: InlineMessage,
    ) -> Result<IpcEffect, EndpointError> {
        if self.closed {
            return Err(EndpointError::Closed);
        }

        match self.state {
            EndpointState::ReplyOutstanding {
                caller,
                token: active_token,
                ..
            } => {
                if token != active_token {
                    return Err(EndpointError::InvalidReplyToken);
                }
                // Transition back to Idle. The caller is woken.
                self.state = EndpointState::Idle;
                Ok(IpcEffect::Wake(caller))
            }
            _ => Err(EndpointError::InvalidReplyToken),
        }
    }

    pub fn close(&mut self) -> EndpointCloseEffects {
        self.closed = true;
        let mut woke_threads = Vec::new();

        match self.state {
            EndpointState::CallerPending { caller, .. } => {
                woke_threads.push(caller);
            }
            EndpointState::ReceiverWaiting { receiver } => {
                woke_threads.push(receiver);
            }
            EndpointState::ReplyOutstanding { caller, .. } => {
                woke_threads.push(caller);
            }
            EndpointState::Idle => {}
        }

        self.state = EndpointState::Idle;
        EndpointCloseEffects { woke_threads }
    }
}

#[derive(Clone, Debug)]
pub struct PreparedMessageTransfers {
    transfers: [Option<PreparedTransfer>; MAX_CAPABILITY_TRANSFERS],
    count: usize,
}

impl PreparedMessageTransfers {
    pub fn prepare(
        descriptors: &[TransferDescriptor],
        source: &CapabilitySpace,
        system: &CapabilitySystem,
        objects: &ObjectArena,
    ) -> Result<Self, CapabilityError> {
        if descriptors.len() > MAX_CAPABILITY_TRANSFERS {
            // Should be bounded by the ABI limit prior to this
            return Err(CapabilityError::SpaceFull); // Reusing an error for out of bounds
        }

        let mut transfers = [None; MAX_CAPABILITY_TRANSFERS];
        let mut count = 0;

        for (i, desc) in descriptors.iter().enumerate() {
            let prep =
                system.prepare_transfer(source, desc.handle, desc.narrowed_rights, objects)?;
            transfers[i] = Some(prep);
            count += 1;
        }

        Ok(Self { transfers, count })
    }

    pub fn commit(
        &self,
        system: &mut CapabilitySystem,
        source: &CapabilitySpace,
        target: &mut CapabilitySpace,
        target_domain: &mut ResourceDomain,
        objects: &ObjectArena,
    ) -> Result<Vec<Handle>, CapabilityError> {
        let mut committed = Vec::with_capacity(self.count);

        for i in 0..self.count {
            if let Some(prep) = self.transfers[i] {
                match system.commit_transfer(source, prep, target, target_domain, objects) {
                    Ok(handle) => {
                        committed.push(handle);
                    }
                    Err(e) => {
                        // Rollback all already committed in this transaction
                        for handle in committed {
                            let _ = system.delete(target, target_domain, handle);
                        }
                        return Err(e);
                    }
                }
            }
        }

        Ok(committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{ResourceDomainId, ResourceLimits};
    use gaxera_abi::{ObjectType, ObjectTypeSet, Rights};

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    fn dummy_message() -> InlineMessage {
        InlineMessage::try_new(&[]).unwrap()
    }

    #[test]
    fn endpoint_caller_first_rendezvous() {
        let mut ep = Endpoint::new(test_id(1));
        let caller = test_id(2);
        let receiver = test_id(3);

        assert_eq!(ep.call(caller, dummy_message()), Ok(IpcEffect::Block));

        let receive_res = ep.receive(receiver).unwrap();
        assert!(receive_res.is_ok()); // Delivered immediately
        let received = receive_res.unwrap();
        assert_eq!(received.caller, caller);

        // Reply completes the rendezvous
        assert_eq!(
            ep.reply(received.reply_token, dummy_message()),
            Ok(IpcEffect::Wake(caller))
        );
    }

    #[test]
    fn endpoint_receiver_first_rendezvous() {
        let mut ep = Endpoint::new(test_id(1));
        let caller = test_id(2);
        let receiver = test_id(3);

        assert_eq!(ep.receive(receiver), Ok(Err(IpcEffect::Block)));

        assert_eq!(
            ep.call(caller, dummy_message()),
            Ok(IpcEffect::Wake(receiver))
        );

        let received = ep
            .take_received_call()
            .expect("Message should be stored for receiver");
        assert_eq!(received.caller, caller);

        assert_eq!(
            ep.reply(received.reply_token, dummy_message()),
            Ok(IpcEffect::Wake(caller))
        );
    }

    #[test]
    fn endpoint_second_caller_rejected() {
        let mut ep = Endpoint::new(test_id(1));
        let caller1 = test_id(2);
        let caller2 = test_id(3);

        assert_eq!(ep.call(caller1, dummy_message()), Ok(IpcEffect::Block));
        assert_eq!(ep.call(caller2, dummy_message()), Err(EndpointError::Busy));
    }

    #[test]
    fn endpoint_second_receiver_rejected() {
        let mut ep = Endpoint::new(test_id(1));
        let receiver1 = test_id(2);
        let receiver2 = test_id(3);

        assert_eq!(ep.receive(receiver1), Ok(Err(IpcEffect::Block)));
        assert_eq!(ep.receive(receiver2), Err(EndpointError::Busy));
    }

    #[test]
    fn endpoint_reply_stale_or_forged() {
        let mut ep = Endpoint::new(test_id(1));
        let caller = test_id(2);
        let receiver = test_id(3);

        assert_eq!(ep.call(caller, dummy_message()), Ok(IpcEffect::Block));
        let received = ep.receive(receiver).unwrap().unwrap();

        let forged_token = ReplyToken::from_raw(received.reply_token.raw() + 1);
        assert_eq!(
            ep.reply(forged_token, dummy_message()),
            Err(EndpointError::InvalidReplyToken)
        );

        // Reply correctly once
        assert_eq!(
            ep.reply(received.reply_token, dummy_message()),
            Ok(IpcEffect::Wake(caller))
        );

        // Reply second time fails
        assert_eq!(
            ep.reply(received.reply_token, dummy_message()),
            Err(EndpointError::InvalidReplyToken)
        );
    }

    #[test]
    fn endpoint_close_effects() {
        let mut ep1 = Endpoint::new(test_id(1));
        assert_eq!(ep1.call(test_id(2), dummy_message()), Ok(IpcEffect::Block));
        assert_eq!(ep1.close().woke_threads, alloc::vec![test_id(2)]);
        assert_eq!(
            ep1.call(test_id(2), dummy_message()),
            Err(EndpointError::Closed)
        );

        let mut ep2 = Endpoint::new(test_id(1));
        assert_eq!(ep2.receive(test_id(3)), Ok(Err(IpcEffect::Block)));
        assert_eq!(ep2.close().woke_threads, alloc::vec![test_id(3)]);

        let mut ep3 = Endpoint::new(test_id(1));
        assert_eq!(ep3.call(test_id(2), dummy_message()), Ok(IpcEffect::Block));
        let received = ep3.receive(test_id(3)).unwrap().unwrap();
        assert_eq!(ep3.close().woke_threads, alloc::vec![test_id(2)]);
        assert_eq!(
            ep3.reply(received.reply_token, dummy_message()),
            Err(EndpointError::Closed)
        );
    }

    fn domain(id: ResourceDomainId, objects: u32, capabilities: u32) -> ResourceDomain {
        ResourceDomain::new(
            id,
            ResourceLimits {
                objects,
                capabilities,
            },
        )
    }

    #[test]
    fn capability_transfer_transaction_all_or_nothing() {
        let mut owner = domain(ResourceDomainId::new(1), 2, 8);
        let mut recipient = domain(ResourceDomainId::new(2), 2, 2); // Recipient has cap limit of 2
        let mut arena = ObjectArena::try_new(2).unwrap();

        let factory = crate::object::Factory::new(&owner, ObjectTypeSet::of(ObjectType::Endpoint));
        let object1 = arena
            .create(&mut owner, factory, ObjectType::Endpoint)
            .unwrap();
        let object2 = arena
            .create(&mut owner, factory, ObjectType::Endpoint)
            .unwrap();

        let mut source = CapabilitySpace::try_new(&owner, 4).unwrap();
        // Target can only hold 1 more capability!
        let mut target = CapabilitySpace::try_new(&recipient, 1).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();

        let h1 = system
            .insert_root(
                &mut source,
                &mut owner,
                object1,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();
        let h2 = system
            .insert_root(
                &mut source,
                &mut owner,
                object2,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();

        let descriptors = [
            TransferDescriptor {
                handle: h1,
                narrowed_rights: Rights::READ,
            },
            TransferDescriptor {
                handle: h2,
                narrowed_rights: Rights::READ,
            },
        ];

        let prepared =
            PreparedMessageTransfers::prepare(&descriptors, &source, &system, &arena).unwrap();

        // Target capacity is 1, so the transaction will fail on the second item and should rollback the first
        let result = prepared.commit(&mut system, &source, &mut target, &mut recipient, &arena);
        assert!(result.is_err());

        // Validate exact rollback: target capability space usage should be 0
        assert_eq!(recipient.usage().capabilities, 0);

        // Re-create target with space for 2 and ensure successful delivery
        let mut target2 = CapabilitySpace::try_new(&recipient, 2).unwrap();
        let result2 = prepared.commit(&mut system, &source, &mut target2, &mut recipient, &arena);
        assert!(result2.is_ok());
        let handles = result2.unwrap();
        assert_eq!(handles.len(), 2);
        assert_eq!(recipient.usage().capabilities, 2);
    }
}
