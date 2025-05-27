//! IOC code definitions.
//!
//! Adapted from the `libc` crate.

const _IOC_NRBITS: u32 = 8;
const _IOC_TYPEBITS: u32 = 8;

#[cfg(any(
    any(target_arch = "powerpc", target_arch = "powerpc64"),
    any(target_arch = "sparc", target_arch = "sparc64"),
    any(target_arch = "mips", target_arch = "mips64"),
))]
mod bits {
    // https://github.com/torvalds/linux/blob/b311c1b497e51a628aa89e7cb954481e5f9dced2/arch/powerpc/include/uapi/asm/ioctl.h
    // https://github.com/torvalds/linux/blob/b311c1b497e51a628aa89e7cb954481e5f9dced2/arch/sparc/include/uapi/asm/ioctl.h
    // https://github.com/torvalds/linux/blob/b311c1b497e51a628aa89e7cb954481e5f9dced2/arch/mips/include/uapi/asm/ioctl.h

    pub const _IOC_SIZEBITS: u32 = 13;
    pub const _IOC_DIRBITS: u32 = 3;

    pub const _IOC_NONE: u32 = 1;
    pub const _IOC_READ: u32 = 2;
    pub const _IOC_WRITE: u32 = 4;
}

#[cfg(not(any(
    any(target_arch = "powerpc", target_arch = "powerpc64"),
    any(target_arch = "sparc", target_arch = "sparc64"),
    any(target_arch = "mips", target_arch = "mips64"),
)))]
mod bits {
    // https://github.com/torvalds/linux/blob/b311c1b497e51a628aa89e7cb954481e5f9dced2/include/uapi/asm-generic/ioctl.h

    pub const _IOC_SIZEBITS: u32 = 14;
    pub const _IOC_DIRBITS: u32 = 2;

    pub const _IOC_NONE: u32 = 0;
    pub const _IOC_WRITE: u32 = 1;
    pub const _IOC_READ: u32 = 2;
}

pub use bits::*;

pub(crate) const _IOC_NRMASK: u32 = (1 << _IOC_NRBITS) - 1;
pub(crate) const _IOC_TYPEMASK: u32 = (1 << _IOC_TYPEBITS) - 1;
pub(crate) const _IOC_SIZEMASK: u32 = (1 << _IOC_SIZEBITS) - 1;
pub(crate) const _IOC_DIRMASK: u32 = (1 << _IOC_DIRBITS) - 1;

pub(crate) const _IOC_NRSHIFT: u32 = 0;
pub(crate) const _IOC_TYPESHIFT: u32 = _IOC_NRSHIFT + _IOC_NRBITS;
pub(crate) const _IOC_SIZESHIFT: u32 = _IOC_TYPESHIFT + _IOC_TYPEBITS;
pub(crate) const _IOC_DIRSHIFT: u32 = _IOC_SIZESHIFT + _IOC_SIZEBITS;

// adapted from https://github.com/torvalds/linux/blob/8a696a29c6905594e4abf78eaafcb62165ac61f1/rust/kernel/ioctl.rs
