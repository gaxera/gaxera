use crate::elf::error::ElfError;
use crate::elf::types::{Elf64_Ehdr, Elf64_Phdr};
use crate::elf::validation::validate_header;
use core::mem::size_of;

pub struct ElfParser<'a> {
    data: &'a [u8],
    header: &'a Elf64_Ehdr,
}

impl<'a> ElfParser<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < size_of::<Elf64_Ehdr>() {
            return Err(ElfError::BufferTooSmall);
        }

        // SAFETY: Limine guarantees boot modules are page-aligned, meaning
        // our byte slice is at least 8-byte aligned, which is sufficient for Elf64_Ehdr.
        let header = unsafe { &*(data.as_ptr() as *const Elf64_Ehdr) };

        validate_header(header)?;

        // Ensure the program header table is entirely within the data buffer
        let phoff = header.e_phoff as usize;
        let phnum = header.e_phnum as usize;
        let phentsize = header.e_phentsize as usize;

        let ph_end = phnum
            .checked_mul(phentsize)
            .and_then(|size| size.checked_add(phoff))
            .ok_or(ElfError::ProgramHeaderOutOfBounds)?;

        if ph_end > data.len() {
            return Err(ElfError::ProgramHeaderOutOfBounds);
        }

        Ok(Self { data, header })
    }

    pub fn header(&self) -> &Elf64_Ehdr {
        self.header
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn program_headers(&self) -> impl Iterator<Item = &'a Elf64_Phdr> {
        let phoff = self.header.e_phoff as usize;
        let phentsize = self.header.e_phentsize as usize;
        let phnum = self.header.e_phnum as usize;

        (0..phnum).map(move |i| {
            let offset = phoff + (i * phentsize);
            // SAFETY: Bounds are verified in `new`.
            unsafe { &*(self.data.as_ptr().add(offset) as *const Elf64_Phdr) }
        })
    }
}
