use crate::elf::error::ElfError;
use crate::elf::types::Elf64_Ehdr;

pub fn validate_header(header: &Elf64_Ehdr) -> Result<(), ElfError> {
    use crate::elf::types::Elf64_Phdr;
    use core::mem::size_of;
    if header.e_ident[0..4] != [0x7f, b'E', b'L', b'F'] {
        return Err(ElfError::InvalidMagic);
    }
    if header.e_ident[4] != 2 {
        // ELFCLASS64
        return Err(ElfError::UnsupportedClass);
    }
    if header.e_ident[5] != 1 {
        // ELFDATA2LSB (Little Endian)
        return Err(ElfError::UnsupportedEndian);
    }
    if header.e_ident[6] != 1 {
        // EV_CURRENT
        return Err(ElfError::UnsupportedVersion);
    }

    if header.e_machine != 0x3E {
        // EM_X86_64
        return Err(ElfError::UnsupportedMachine);
    }
    if header.e_version != 1 {
        return Err(ElfError::UnsupportedVersion);
    }

    if header.e_ehsize as usize != size_of::<Elf64_Ehdr>() {
        return Err(ElfError::MalformedHeaderSize);
    }

    if header.e_phentsize as usize != size_of::<Elf64_Phdr>() {
        return Err(ElfError::MalformedHeaderSize);
    }

    Ok(())
}
