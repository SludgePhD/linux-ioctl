//! Platform details for Linux and Android.

#[cfg(any(
    target_arch = "mips",
    target_arch = "mips64",
    target_arch = "sparc",
    target_arch = "sparc64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    //target_arch = "alpha",
))]
mod consts {
    pub(crate) const _IOC_SIZEBITS: u32 = 13;

    pub(crate) const _IOC_NONE: u32 = 1;
    pub(crate) const _IOC_READ: u32 = 2;
    pub(crate) const _IOC_WRITE: u32 = 4;
}

#[cfg(not(any(
    target_arch = "mips",
    target_arch = "mips64",
    target_arch = "sparc",
    target_arch = "sparc64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    //target_arch = "alpha",
)))]
mod consts {
    pub(crate) const _IOC_SIZEBITS: u32 = 14;

    pub(crate) const _IOC_NONE: u32 = 0;
    pub(crate) const _IOC_READ: u32 = 2;
    pub(crate) const _IOC_WRITE: u32 = 1;
}

use consts::_IOC_SIZEBITS;

const _IOC_NRBITS: u32 = 8;
const _IOC_TYPEBITS: u32 = 8;

const _IOC_NRSHIFT: u32 = 0;
const _IOC_TYPESHIFT: u32 = _IOC_NRSHIFT + _IOC_NRBITS;
const _IOC_SIZESHIFT: u32 = _IOC_TYPESHIFT + _IOC_TYPEBITS;
const _IOC_DIRSHIFT: u32 = _IOC_SIZESHIFT + _IOC_SIZEBITS;

pub(crate) use consts::{_IOC_NONE, _IOC_READ, _IOC_WRITE};

/// The largest argument size that can be portably encoded.
pub(crate) const MAX_ARG_SIZE: usize = (1 << 13) - 1;

#[expect(non_snake_case)]
pub(crate) const fn _IOC(dir: u32, ty: u32, nr: u32, size: u32) -> u32 {
    dir << _IOC_DIRSHIFT | ty << _IOC_TYPESHIFT | nr << _IOC_NRSHIFT | size << _IOC_SIZESHIFT
}
