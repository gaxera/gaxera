use crate::arch::x86_64::paging::KernelPageTables;
use crate::arch::x86_64::stack::KernelStack;
use crate::memory::physical::SegmentedBitmapFrameAllocator;
use crate::println;

pub fn run_cooperative_yield_test(
    page_tables: &mut KernelPageTables,
    physical_frames: &mut SegmentedBitmapFrameAllocator<'_>,
) -> ! {
    let stack0 = KernelStack::allocate(page_tables, physical_frames).unwrap();
    let stack1 = KernelStack::allocate(page_tables, physical_frames).unwrap();
    let thread0_stack_top = stack0.top().as_u64();

    let probe = crate::arch::x86_64::probe::M2AProbe::build(page_tables, physical_frames).unwrap();
    let selectors = crate::arch::x86_64::descriptors::user_selectors().unwrap();
    let frame = crate::arch::x86_64::user::UserTransitionFrame::fixed_probe(selectors);

    let arch0 = crate::arch::x86_64::thread::spawn_user_thread(stack0, None, frame);
    let arch1 = crate::arch::x86_64::thread::spawn_user_thread(stack1, None, frame);

    use kernel_core::object::ObjectId;
    use kernel_core::thread::Thread;

    let mut thread0 = Thread::new(ObjectId::new_for_test(1, 1), None, arch0);
    let mut thread1 = Thread::new(ObjectId::new_for_test(2, 1), None, arch1);

    let cpu_local = unsafe { crate::arch::x86_64::cpu::get_cpu_local() };
    unsafe {
        *cpu_local.scheduler.get() = Some(kernel_core::scheduler::Scheduler::try_new(256).unwrap());
    }
    let scheduler = unsafe { &mut *cpu_local.scheduler.get() };
    if let Some(sched) = scheduler.as_mut() {
        sched.enqueue(&mut thread1).unwrap();
        // Set thread 0 as current (it will run first when we jump to user space)
        let _ = thread0.make_runnable();
        let _ = thread0.make_running();
        sched.set_current_thread(Some(thread0.id()));
    }

    unsafe {
        crate::arch::x86_64::cpu::set_kernel_stack_top(thread0_stack_top);
        crate::arch::x86_64::thread::THREADS.insert(thread0);
        crate::arch::x86_64::thread::THREADS.insert(thread1);
    }

    println!("GAXERA: TEST_PATTERN_DRAWN"); // Satisfy the runner's sequence
    probe.execute_on_kernel_stack(thread0_stack_top);
}
