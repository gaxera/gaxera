use crate::elf::error::ElfError;
use crate::elf::types::{Elf64_Ehdr, Elf64_Phdr};
use crate::elf::validation::validate_header;
use core::mem::size_of;

#[derive(Debug)]
pub struct ElfParser<'a> {
    data: &'a [u8],
    header: &'a Elf64_Ehdr,
}

impl<'a> ElfParser<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < size_of::<Elf64_Ehdr>() {
            return Err(ElfError::BufferTooSmall);
        }

        if data.as_ptr().align_offset(8) != 0 {
            return Err(ElfError::InvalidAlignment);
        }

        // SAFETY: `data` length is verified >= size_of::<Elf64_Ehdr>() and pointer
        // alignment is verified to meet `Elf64_Ehdr` requirements above.
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

        // SAFETY: Verify alignment for program headers so we can safely cast them later.
        // We know `phoff` fits in bounds, but we must ensure the pointer is aligned.
        unsafe {
            if data.as_ptr().add(phoff).align_offset(8) != 0 {
                return Err(ElfError::InvalidAlignment);
            }
        }

        let parser = Self { data, header };

        // Validate all program headers for arithmetic overflow and bounds
        for ph in parser.program_headers() {
            if ph.p_type == crate::elf::types::PT_LOAD {
                if ph.p_filesz > ph.p_memsz {
                    return Err(ElfError::ProgramHeaderOutOfBounds);
                }

                let segment_end = ph
                    .p_offset
                    .checked_add(ph.p_filesz)
                    .ok_or(ElfError::ProgramHeaderOutOfBounds)?;

                if segment_end > data.len() as u64 {
                    return Err(ElfError::ProgramHeaderOutOfBounds);
                }
            }
        }

        Ok(parser)
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
            // We use checked math here for safety, though `new()` guarantees `i * phentsize + phoff` won't overflow.
            let offset = i
                .checked_mul(phentsize)
                .and_then(|o| o.checked_add(phoff))
                .unwrap();

            // SAFETY: `new()` validates that `phoff + (phnum * phentsize)` <= `data.len()`.
            // `e_phentsize` is strictly validated to equal `size_of::<Elf64_Phdr>()`.
            // Alignment of `data.as_ptr().add(phoff)` is also strictly verified in `new()`.
            unsafe { &*(self.data.as_ptr().add(offset) as *const Elf64_Phdr) }
        })
    }
}
