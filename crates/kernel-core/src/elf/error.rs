#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElfError {
    BufferTooSmall,
    InvalidMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedVersion,
    UnsupportedAbi,
    UnsupportedMachine,
    ProgramHeaderOutOfBounds,
    MalformedHeaderSize,
    InvalidAlignment,
}
