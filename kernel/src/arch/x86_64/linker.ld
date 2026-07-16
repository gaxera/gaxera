/* Custom Higher Half ELF64 Linker Script for Gaxera */
OUTPUT_FORMAT("elf64-x86-64")
ENTRY(_start)

/*
 * Program Headers (Segments)
 * Tells the bootloader loader how to group ELF sections into memory segments
 * with different page protections.
 */
PHDRS
{
    text    PT_LOAD    FLAGS(5); /* Read and Execute (R-X) - Kernel Code */
    rodata  PT_LOAD    FLAGS(4); /* Read Only (R--) - Constants */
    data    PT_LOAD    FLAGS(6); /* Read and Write (RW-) - Initialized Variables */
}

SECTIONS
{
    /*
     * Link-Time Virtual Memory Address (VMA)
     * Maps the kernel in the virtual Higher Half (last 2 GiB).
     *
     * Note: The virtual base of 0xffffffff80000000 is chosen to comply with
     * the x86-64 'kernel' code model (-mcmodel=kernel), which optimizes symbol
     * addressing. Limine's ELF loader maps the sections to this VMA using
     * physical page frames allocated dynamically at boot.
     */
    . = 0xffffffff80000000;

    __kernel_start = .;

    /* Executable Code Segment */
    .text : {
        __text_start = .;
        *(.text .text.*)
        __text_end = .;
    } :text

    . = ALIGN(4K);

    /* Read-Only Data Segment */
    .rodata : {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    } :rodata

    /* Each PT_LOAD segment must own whole pages; Limine rejects an R--/RW-
     * permission split within a single 4 KiB page. */
    . = ALIGN(4K);

    /* Limine scans this writable segment for used request records. */
    .requests ALIGN(4K) : {
        __requests_start = .;
        KEEP(*(.requests .requests.*))
        KEEP(*(.limine_requests .limine_requests.*))
        __requests_end = .;
    } :data

    . = ALIGN(4K);

    /* Read-Write Initialized Data */
    .data : {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } :data

    /* Read-Write Uninitialized Data (Zeroed at Boot) */
    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    /* These stacks get lower guard pages once Gaxera owns CR3. The pages are
     * allocated by the ELF image today but deliberately left unmapped later. */
    . = ALIGN(4K);
    __bootstrap_stack_guard_start = .;
    . += 4K;
    __bootstrap_stack_guard_end = .;
    .bootstrap_stack : {
        __bootstrap_stack_start = .;
        KEEP(*(.bootstrap_stack))
        __bootstrap_stack_end = .;
    } :data

    . = ALIGN(4K);
    __ist_stack_guard_start = .;
    . += 4K;
    __ist_stack_guard_end = .;
    .ist_stack : {
        __ist_stack_start = .;
        KEEP(*(.ist_stack))
        __ist_stack_end = .;
    } :data

    __kernel_end = .;

    /* Discard compiler-generated stack unwinding metadata */
    /DISCARD/ : {
        *(.eh_frame)
        *(.note.GNU-stack)
    }
}
