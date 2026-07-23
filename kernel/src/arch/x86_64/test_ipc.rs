use crate::println;
use gaxera_abi::ipc::{InlineMessage, TransferDescriptor};
use gaxera_abi::{ObjectType, ObjectTypeSet, Rights};
use kernel_core::capability::{CapabilitySpace, CapabilitySystem};
use kernel_core::ipc::{Endpoint, EndpointError, IpcEffect, PreparedMessageTransfers};
use kernel_core::notification::Notification;
use kernel_core::object::{Factory, ObjectArena, ObjectId};
use kernel_core::resource::{ResourceDomain, ResourceDomainId, ResourceLimits};

fn dummy_message(val: u8) -> InlineMessage {
    let mut bytes = [0; 64];
    bytes[0] = val;
    InlineMessage::try_new(&bytes[0..1]).unwrap()
}

pub fn run_ipc_test() -> ! {
    let mut arena = ObjectArena::try_new(16).unwrap();
    let mut system = CapabilitySystem::try_new(16).unwrap();

    let mut owner = ResourceDomain::new_for_test(
        ResourceDomainId::new_for_test(1),
        ResourceLimits {
            objects: 16,
            capabilities: 16,
        },
    );
    let mut source_space = CapabilitySpace::try_new(&owner, 8).unwrap();

    // Test 1: Endpoint call/receive/reply rendezvous
    let ep_factory = Factory::new_for_test(&owner, ObjectTypeSet::of(ObjectType::Endpoint));
    let ep_id = arena
        .create(&mut owner, ep_factory, ObjectType::Endpoint)
        .unwrap();

    // We can interact directly with the state machine
    let mut ep = Endpoint::new(ep_id);
    let caller_id = ObjectId::new_for_test(100, 1);
    let receiver_id = ObjectId::new_for_test(200, 1);

    assert_eq!(ep.call(caller_id, dummy_message(1)), Ok(IpcEffect::Block));
    let received = ep.receive(receiver_id).unwrap().unwrap();
    assert_eq!(received.message.payload()[0], 1);

    assert_eq!(
        ep.reply(received.reply_token, dummy_message(2)),
        Ok(IpcEffect::Wake(caller_id))
    );

    // Test 2: Endpoint denial (busy)
    assert_eq!(ep.receive(receiver_id), Ok(Err(IpcEffect::Block)));
    assert_eq!(
        ep.receive(ObjectId::new_for_test(201, 1)),
        Err(EndpointError::Busy)
    );

    let woke = ep.close();
    assert_eq!(woke.woke_threads, alloc::vec![receiver_id]);

    // Test 3: Transfer & exact rollback
    let mut recipient = ResourceDomain::new_for_test(
        ResourceDomainId::new_for_test(2),
        ResourceLimits {
            objects: 4,
            capabilities: 1, // Target space limit is 1
        },
    );
    let mut target_space = CapabilitySpace::try_new(&recipient, 1).unwrap();

    let obj1_id = arena
        .create(&mut owner, ep_factory, ObjectType::Endpoint)
        .unwrap();
    let obj2_id = arena
        .create(&mut owner, ep_factory, ObjectType::Endpoint)
        .unwrap();

    let h1 = system
        .insert_root(
            &mut source_space,
            &mut owner,
            obj1_id,
            ObjectType::Endpoint,
            Rights::READ,
            &arena,
        )
        .unwrap();
    let h2 = system
        .insert_root(
            &mut source_space,
            &mut owner,
            obj2_id,
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
        PreparedMessageTransfers::prepare(&descriptors, &source_space, &system, &arena).unwrap();

    // Rollback validation
    assert!(
        prepared
            .commit(
                &mut system,
                &source_space,
                &mut target_space,
                &mut recipient,
                &arena
            )
            .is_err()
    );
    assert_eq!(recipient.usage().capabilities, 0); // Exact rollback

    // Test 4: Notification signal and take
    let mut notif = Notification::new(ObjectId::new_for_test(300, 1));
    notif.signal(0b1010);
    notif.signal(0b0101);
    assert_eq!(notif.take_signals(), 0b1111);

    println!("GAXERA: IPC_TEST_OK");

    #[cfg(feature = "qemu-test")]
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        crate::arch::x86_64::qemu::exit_success();
    }

    #[cfg(not(feature = "qemu-test"))]
    crate::serial::idle()
}
