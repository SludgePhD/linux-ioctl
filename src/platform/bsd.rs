//! Platform details for BSD-derivatives.

const IOCPARM_SHIFT: u32 = 13;

pub(crate) const MAX_ARG_SIZE: usize = (1 << IOCPARM_SHIFT) - 1;

pub(crate) const IOC_VOID: u32 = 0x20000000;
pub(crate) const IOC_OUT: u32 = 0x40000000;
pub(crate) const IOC_IN: u32 = 0x80000000;

pub(crate) use IOC_IN as _IOC_WRITE;
pub(crate) use IOC_OUT as _IOC_READ;
pub(crate) use IOC_VOID as _IOC_NONE;

#[expect(non_snake_case)]
pub(crate) const fn _IOC(dir: u32, group: u32, num: u32, len: u32) -> u32 {
    dir | len << 16 | group << 8 | num
}
