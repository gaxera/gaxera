use super::*;
use core::mem::size_of;

#[repr(C, align(8))]
struct AlignedBuffer<const N: usize>(pub [u8; N]);

fn create_valid_elf_header() -> Elf64_Ehdr {
    Elf64_Ehdr {
        e_ident: [0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        e_type: 2,       // EXEC
        e_machine: 0x3E, // x86_64
        e_version: 1,
        e_entry: 0x400000,
        e_phoff: size_of::<Elf64_Ehdr>() as u64,
        e_shoff: 0,
        e_flags: 0,
        e_ehsize: size_of::<Elf64_Ehdr>() as u16,
        e_phentsize: size_of::<Elf64_Phdr>() as u16,
        e_phnum: 1,
        e_shentsize: 0,
        e_shnum: 0,
        e_shstrndx: 0,
    }
}

fn create_valid_program_header() -> Elf64_Phdr {
    Elf64_Phdr {
        p_type: PT_LOAD,
        p_flags: PF_R | PF_X,
        p_offset: (size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>()) as u64,
        p_vaddr: 0x400000,
        p_paddr: 0x400000,
        p_filesz: 0x1000,
        p_memsz: 0x1000,
        p_align: 0x1000,
    }
}

#[test]
fn test_valid_elf() {
    let ehdr = create_valid_elf_header();
    let phdr = create_valid_program_header();

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>() + 0x1000;

    let mut aligned = AlignedBuffer::<10000>([0; 10000]);
    let data = &mut aligned.0[..total_size];

    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(data.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        core::ptr::write(
            data.as_mut_ptr().add(size_of::<Elf64_Ehdr>()) as *mut Elf64_Phdr,
            phdr,
        );
    }

    let parser = ElfParser::new(data).expect("Should parse valid ELF");
    let e_phnum = parser.header().e_phnum;
    assert_eq!(e_phnum, 1);

    let mut ph_iter = parser.program_headers();
    let parsed_ph = ph_iter.next().unwrap();
    let p_type = parsed_ph.p_type;
    let p_vaddr = parsed_ph.p_vaddr;

    assert_eq!(p_type, PT_LOAD);
    assert_eq!(p_vaddr, 0x400000);
}

#[test]
fn test_wx_simultaneous_permission_rejection() {
    let ehdr = create_valid_elf_header();
    let mut phdr = create_valid_program_header();
    phdr.p_flags = PF_R | PF_W | PF_X; // Simultaneous W^X violation

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>() + 0x1000;
    let mut aligned = AlignedBuffer::<10000>([0; 10000]);
    let data = &mut aligned.0[..total_size];

    unsafe {
        core::ptr::write(data.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        core::ptr::write(
            data.as_mut_ptr().add(size_of::<Elf64_Ehdr>()) as *mut Elf64_Phdr,
            phdr,
        );
    }

    let err = ElfParser::new(data).unwrap_err();
    assert_eq!(err, ElfError::InvalidAlignment);
}

#[test]
fn test_truncated_buffer_smaller_than_header() {
    let aligned = AlignedBuffer::<16>([0; 16]);
    let err = ElfParser::new(&aligned.0).unwrap_err();
    assert_eq!(err, ElfError::BufferTooSmall);
}

#[test]
fn test_invalid_alignment() {
    let mut ehdr = create_valid_elf_header();
    ehdr.e_phnum = 0; // Prevent ProgramHeaderOutOfBounds
    let mut data = [0u8; 100];

    let ptr = data.as_mut_ptr();
    let offset = if ptr.align_offset(8) == 0 { 1 } else { 0 };
    let unaligned_slice = &mut data[offset..offset + size_of::<Elf64_Ehdr>()];

    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write_unaligned(unaligned_slice.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
    }

    if unaligned_slice.as_ptr().align_offset(8) != 0 {
        let err = ElfParser::new(unaligned_slice).unwrap_err();
        assert_eq!(err, ElfError::InvalidAlignment);
    }
}

#[test]
fn test_invalid_ehsize() {
    let mut ehdr = create_valid_elf_header();
    ehdr.e_ehsize = (size_of::<Elf64_Ehdr>() - 1) as u16;
    let mut aligned = AlignedBuffer::<100>([0; 100]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
    }

    let err = ElfParser::new(&aligned.0[..size_of::<Elf64_Ehdr>()]).unwrap_err();
    assert_eq!(err, ElfError::MalformedHeaderSize);
}

#[test]
fn test_invalid_phentsize() {
    let mut ehdr = create_valid_elf_header();
    ehdr.e_phentsize = 1;
    let mut aligned = AlignedBuffer::<100>([0; 100]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
    }

    let err = ElfParser::new(&aligned.0[..size_of::<Elf64_Ehdr>()]).unwrap_err();
    assert_eq!(err, ElfError::MalformedHeaderSize);
}

#[test]
fn test_truncated_program_headers() {
    let mut ehdr = create_valid_elf_header();
    ehdr.e_phnum = 2;

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>();
    let mut aligned = AlignedBuffer::<1000>([0; 1000]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
    }

    let err = ElfParser::new(&aligned.0[..total_size]).unwrap_err();
    assert_eq!(err, ElfError::ProgramHeaderOutOfBounds);
}

#[test]
fn test_malformed_pt_load_filesz_greater_than_memsz() {
    let ehdr = create_valid_elf_header();
    let mut phdr = create_valid_program_header();
    phdr.p_filesz = 0x2000;
    phdr.p_memsz = 0x1000;

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>() + 0x2000;
    let mut aligned = AlignedBuffer::<10000>([0; 10000]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        core::ptr::write(
            aligned.0.as_mut_ptr().add(size_of::<Elf64_Ehdr>()) as *mut Elf64_Phdr,
            phdr,
        );
    }

    let err = ElfParser::new(&aligned.0[..total_size]).unwrap_err();
    assert_eq!(err, ElfError::ProgramHeaderOutOfBounds);
}

#[test]
fn test_malformed_pt_load_out_of_bounds_file_offset() {
    let ehdr = create_valid_elf_header();
    let mut phdr = create_valid_program_header();
    phdr.p_offset = 5000;
    phdr.p_filesz = 0x1000;

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>();
    let mut aligned = AlignedBuffer::<10000>([0; 10000]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        core::ptr::write(
            aligned.0.as_mut_ptr().add(size_of::<Elf64_Ehdr>()) as *mut Elf64_Phdr,
            phdr,
        );
    }

    let err = ElfParser::new(&aligned.0[..total_size]).unwrap_err();
    assert_eq!(err, ElfError::ProgramHeaderOutOfBounds);
}

#[test]
fn test_malformed_pt_load_arithmetic_overflow() {
    let ehdr = create_valid_elf_header();
    let mut phdr = create_valid_program_header();
    phdr.p_offset = u64::MAX;
    phdr.p_filesz = 0x1000;

    let total_size = size_of::<Elf64_Ehdr>() + size_of::<Elf64_Phdr>() + 0x1000;
    let mut aligned = AlignedBuffer::<10000>([0; 10000]);
    // SAFETY: Writing to a valid, appropriately sized local test buffer.
    unsafe {
        core::ptr::write(aligned.0.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        core::ptr::write(
            aligned.0.as_mut_ptr().add(size_of::<Elf64_Ehdr>()) as *mut Elf64_Phdr,
            phdr,
        );
    }

    let err = ElfParser::new(&aligned.0[..total_size]).unwrap_err();
    assert_eq!(err, ElfError::ProgramHeaderOutOfBounds);
}
