//! `ioctl`s for Linux APIs.
//!
//! This library provides a convenient way to bind to Linux `ioctl`s.
//!
//! It is intended to help with writing wrappers around driver functionality, and tries to mirror
//! the syntax you'll find in C headers closely.
//!
//! # Example
//!
//! Let's wrap V4L2's `QUERYCAP` ioctl.
//!
//! From `linux/videodev2.h`:
//!
//! ```c
//! struct v4l2_capability {
//! 	__u8	driver[16];
//! 	__u8	card[32];
//! 	__u8	bus_info[32];
//! 	__u32   version;
//! 	__u32	capabilities;
//! 	__u32	device_caps;
//! 	__u32	reserved[3];
//! };
//! // ...
//! #define VIDIOC_QUERYCAP		 _IOR('V',  0, struct v4l2_capability)
//! ```
//!
//! ```no_run
//! use std::mem::MaybeUninit;
//! use linux_ioctl::*;
//!
//! #[repr(C)]
//! struct Capability {
//!     driver: [u8; 16],
//!     card: [u8; 32],
//!     bus_info: [u8; 32],
//!     version: u32,
//!     capabilities: u32,
//!     device_caps: u32,
//!     reserved: [u32; 3],
//! }
//!
//! const VIDIOC_QUERYCAP: Ioctl<*mut Capability> = _IOR(b'V', 0);
//!
//! // Use as follows:
//!
//! # let fd = 123;
//! let capability = unsafe {
//!     let mut capability = MaybeUninit::uninit();
//!     VIDIOC_QUERYCAP.ioctl(&fd, capability.as_mut_ptr())?;
//!     capability.assume_init()
//! };
//! # std::io::Result::Ok(())
//! ```
//!
//! # Portability
//!
//! Despite being about Linux APIs, and following the Linux convention for declaring *ioctl* codes,
//! this library should also work on other operating systems that implement a Linux-comparible
//! `ioctl`-based API.
//!
//! For example, FreeBSD implements a variety of compatible interfaces like *evdev* and *V4L2*.
//!
//! # Safety
//!
//! To safely perform an *ioctl*, the actual behavior of the kernel-side has to match the behavior
//! expected by userspace (which is encoded in the [`Ioctl`] type).
//!
//! To accomplish this, it is necessary that the [`Ioctl`] was constructed correctly by the caller:
//! the direction, type, number, and argument type size are used to build the ioctl request code,
//! and the Rust type used as the ioctl argument has to match what the kernel expects.
//! If the argument is a pointer the kernel will read from or write to, the data behind the pointer
//! also has to be valid, of course (`ioctl`s are arbitrary functions, so the same care is needed
//! as when binding to an arbitrary C function).
//!
//! However, this is not, strictly speaking, *sufficient* to ensure safety:
//! several drivers and subsystems share the same *ioctl* "type" value, which may lead to an ioctl
//! request code that is interpreted differently, depending on which driver receives the request.
//! Since the *ioctl* request code encodes the size of the argument type, this operation is unlikely
//! to cause a fault when accessing memory, since both argument types have the same size, so the
//! `ioctl` syscall may complete successfully instead of returning `EFAULT`.
//!
//! The result of this situation is that a type intended for data from one driver now has data from
//! an entirely unrelated driver in it, which will likely cause UB, either because a *validity
//! invariant* was violated by the data written to the structure, or because userspace will trust
//! the kernel to only write valid data (including pointers) to the structure.
//!
//! While it may technically be possible to tell which driver owns a given device file descriptor
//! by crawling `/sys` or querying `udev`, in practice this situation is deemed "sufficiently
//! unlikely to cause problems" and programs don't bother with this.
//!
//! One way to rule out this issue is to prevent arbitrary file descriptors from making their way
//! to the ioctl, and to ensure that only files that match the driver's naming convention are used
//! for these ioctls.
//! For example, an *evdev* wrapper could refuse to operate on files outside of `/dev/input`, and a
//! KVM API could always open `/dev/kvm` without offering a safe API to act on a different device
//! file.
//!
//! For more information, you can look at the list of ioctl groups here:
//! <https://www.kernel.org/doc/html/latest/userspace-api/ioctl/ioctl-number.html>
//!
//! ***TL;DR**: don't worry about it kitten :)*

#[doc = include_str!("../README.md")]
mod readme {}

mod consts;

use std::{ffi::c_int, io, marker::PhantomData, os::fd::AsRawFd};

use consts::_IOC_SIZEMASK;

/// An *ioctl*.
///
/// [`Ioctl`] can represent *ioctl*s that take either no arguments or a single argument.
/// If `T` is [`NoArgs`], the *ioctl* takes no arguments.
/// For other values of `T`, the *ioctl* takes `T` as its only argument.
/// Often, the argument `T` is a pointer or reference to a struct that contains the actual
/// arguments.
///
/// While [`Ioctl`] cannot handle *ioctl*s that require passing more than one argument to the
/// `ioctl(2)` function, Linux doesn't have any *ioctl*s that take more than one argument, and is
/// unlikely to gain any in the future.
///
/// The [`Ioctl`] type is constructed with the free functions [`_IO`], [`_IOR`], [`_IOW`],
/// [`_IOWR`], and [`_IOC`].
/// For legacy *ioctl*s, it can also be created via [`Ioctl::from_raw`].
pub struct Ioctl<T: ?Sized = NoArgs> {
    request: u32,
    _p: PhantomData<T>,
}

impl<T: ?Sized> Copy for Ioctl<T> {}
impl<T: ?Sized> Clone for Ioctl<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Ioctl<T> {
    /// Creates an [`Ioctl`] object from a raw request code and an arbitrary argument type.
    ///
    /// This can be used for legacy *ioctl*s that were defined before the `_IOx` macros were
    /// introduced.
    ///
    /// # Examples
    ///
    /// From `asm-generic/ioctls.h`:
    ///
    /// ```c
    /// #define FIONREAD	0x541B
    /// ```
    ///
    /// From `man 2const FIONREAD`:
    ///
    /// ```text
    /// DESCRIPTION
    ///     FIONREAD
    ///         Get the number of bytes in the input buffer.
    ///     ...
    /// SYNOPSIS
    ///     ...
    ///     int ioctl(int fd, FIONREAD, int *argp);
    ///     ...
    /// ```
    ///
    /// ```
    /// use std::fs::File;
    /// use std::ffi::c_int;
    /// use linux_ioctl::*;
    ///
    /// const FIONREAD: Ioctl<*mut c_int> = Ioctl::from_raw(0x541B);
    ///
    /// let file = File::open("/dev/tty")?;
    ///
    /// let mut bytes = c_int::MAX;
    /// unsafe { FIONREAD.ioctl(&file, &mut bytes)? };
    /// assert_ne!(bytes, c_int::MAX);
    ///
    /// println!("{} bytes in input buffer", bytes);
    /// # std::io::Result::Ok(())
    /// ```
    pub const fn from_raw(request: u32) -> Self {
        Self {
            request,
            _p: PhantomData,
        }
    }

    /// Changes the *ioctl* argument type to `T2`.
    ///
    /// This can be used for *ioctl*s that incorrectly declare their type, or for *ioctl*s that take
    /// a by-value argument, rather than [`_IOW`]-type *ioctl*s that take their argument indirectly
    /// through a pointer.
    ///
    /// Returns an [`Ioctl`] that passes an argument of type `T2` to the kernel, while using the
    /// *ioctl* request code from `self`.
    ///
    /// # Examples
    ///
    /// The `KVM_CREATE_VM` *ioctl* is declared with [`_IO`], but takes an `int` as its argument,
    /// specifying the VM type (`KVM_VM_*`).
    ///
    /// From `linux/kvm.h`:
    ///
    /// ```c
    /// #define KVMIO 0xAE
    /// ...
    /// #define KVM_CREATE_VM             _IO(KVMIO,   0x01) /* returns a VM fd */
    /// ```
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use std::ffi::c_int;
    /// use linux_ioctl::*;
    ///
    /// const KVMIO: u8 = 0xAE;
    /// const KVM_CREATE_VM: Ioctl<c_int> = _IO(KVMIO, 0x01).with_arg::<c_int>();
    ///
    /// // The `KVM_CREATE_VM` ioctl takes the VM type as an argument. 0 is a reasonable default on
    /// // most architectures.
    /// let vm_type: c_int = 0;
    ///
    /// let file = File::open("/dev/kvm")?;
    ///
    /// let vm_fd = unsafe { KVM_CREATE_VM.ioctl(&file, vm_type)? };
    /// println!("created new VM with file descriptor {vm_fd}");
    ///
    /// unsafe { libc::close(vm_fd) };
    /// # std::io::Result::Ok(())
    /// ```
    pub const fn with_arg<T2>(self) -> Ioctl<T2> {
        Ioctl {
            request: self.request,
            _p: PhantomData,
        }
    }

    /// Returns the *ioctl* request code.
    ///
    /// This is passed to `ioctl(2)` as its second argument.
    pub fn request(self) -> u32 {
        self.request
    }
}

impl Ioctl<NoArgs> {
    /// Performs an *ioctl* that doesn't take an argument.
    ///
    /// On success, returns the value returned by the `ioctl` syscall. On error (when `ioctl`
    /// returns -1), returns the error from *errno*.
    ///
    /// Note that the actual `ioctl(2)` call performed will pass 0 as a dummy argument to the
    /// *ioctl*. This is because some Linux *ioctl*s are declared without an argument, but will fail
    /// unless they receive 0 as their argument (eg. `KVM_GET_API_VERSION`). There should be no harm
    /// in passing this argument unconditionally, as the kernel will typically just ignore excess
    /// arguments.
    ///
    /// # Safety
    ///
    /// This method performs an arbitrary *ioctl* on an arbitrary file descriptor.
    /// The caller has to ensure that any safety requirements of the *ioctl* are met, and that `fd`
    /// belongs to the driver it expects.
    pub unsafe fn ioctl(self, fd: &impl AsRawFd) -> io::Result<c_int> {
        let res = unsafe { libc::ioctl(fd.as_raw_fd(), self.request.into(), 0) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }
}

impl<T> Ioctl<T> {
    /// Performs an *ioctl* that takes an argument of type `T`.
    ///
    /// Returns the value returned by the `ioctl(2)` invocation, or an I/O error if the call failed.
    ///
    /// For many *ioctl*s, `T` will be a pointer to the actual argument.
    /// The caller must ensure that it points to valid data that conforms to the requirements of the
    /// *ioctl*.
    ///
    /// # Safety
    ///
    /// This method performs an arbitrary *ioctl* on an arbitrary file descriptor.
    /// The caller has to ensure that any safety requirements of the *ioctl* are met, and that `fd`
    /// belongs to the driver it expects.
    pub unsafe fn ioctl(self, fd: &impl AsRawFd, arg: T) -> io::Result<c_int> {
        let res = unsafe { libc::ioctl(fd.as_raw_fd(), self.request.into(), arg) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }
}

/// Indicates that an [`Ioctl`] does not take any arguments.
///
/// This is used as the type parameter of [`Ioctl`] by the [`_IO`] and [`_IOC`] functions.
/// [`Ioctl<NoArgs>`] comes with its own, separate `IOCTL.ioctl(fd)` method that only takes the file
/// descriptor as an argument.
///
/// Since [`NoArgs`] is the default value for [`Ioctl`]'s type parameter, it can typically be
/// omitted.
///
/// # Example
///
/// The *uinput* ioctls `UI_DEV_CREATE` and `UI_DEV_DESTROY` do not take any arguments, while
/// `UI_DEV_SETUP` *does* take an argument.
///
/// From `linux/uinput.h`:
///
/// ```c
/// /* ioctl */
/// #define UINPUT_IOCTL_BASE	'U'
/// #define UI_DEV_CREATE		_IO(UINPUT_IOCTL_BASE, 1)
/// #define UI_DEV_DESTROY		_IO(UINPUT_IOCTL_BASE, 2)
/// ...
/// #define UI_DEV_SETUP _IOW(UINPUT_IOCTL_BASE, 3, struct uinput_setup)
/// ```
///
/// ```rust
/// use std::{mem, fs::File, ffi::c_char};
/// use libc::uinput_setup;
/// use linux_ioctl::*;
///
/// const UINPUT_IOCTL_BASE: u8 = b'U';
/// const UI_DEV_CREATE: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 1);
/// const UI_DEV_DESTROY: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 2);
/// const UI_DEV_SETUP: Ioctl<*const uinput_setup> = _IOW(UINPUT_IOCTL_BASE, 3);
///
/// let uinput = File::options().write(true).open("/dev/uinput")?;
///
/// let mut setup: libc::uinput_setup = unsafe { mem::zeroed() };
/// setup.name[0] = b'A' as c_char; // (must not be blank)
/// unsafe {
///     UI_DEV_SETUP.ioctl(&uinput, &setup)?;
///     UI_DEV_CREATE.ioctl(&uinput)?;
///     // ...use the device...
///     UI_DEV_DESTROY.ioctl(&uinput)?;
/// }
/// # std::io::Result::Ok(())
/// ```
pub struct NoArgs {
    // Unsized type so that the `impl<T> Ioctl<T>` does not conflict.
    _f: [u8],
}

/// Indicates that an *ioctl* neither reads nor writes data through its argument.
pub const _IOC_NONE: u32 = consts::_IOC_NONE;

/// Indicates that an *ioctl* reads data through its pointer argument.
pub const _IOC_READ: u32 = consts::_IOC_READ;

/// Indicates that an *ioctl* writes data through its pointer argument.
pub const _IOC_WRITE: u32 = consts::_IOC_WRITE;

// NB: these are bare `u32`s because `const` `BitOr` impls aren't possible on stable
// (they're only used with `_IOC`, which is a somewhat niche API)

/// Creates an [`Ioctl`] that doesn't read or write any userspace data.
///
/// This type of ioctl can return an `int` to userspace via the return value of the `ioctl` syscall.
/// By default, the returned [`Ioctl`] takes no argument.
/// [`Ioctl::with_arg`] can be used to pass a direct argument to the *ioctl*.
///
/// # Example
///
/// `KVM_GET_API_VERSION` is an *ioctl* that does not take any arguments. The API version is
/// returned as the return value of the `ioctl(2)` function.
///
/// From `linux/kvm.h`:
///
/// ```c
/// #define KVMIO 0xAE
/// ...
/// #define KVM_GET_API_VERSION       _IO(KVMIO,   0x00)
/// ```
///
/// ```rust
/// use std::fs::File;
/// use linux_ioctl::*;
///
/// const KVMIO: u8 = 0xAE;
/// const KVM_GET_API_VERSION: Ioctl<NoArgs> = _IO(KVMIO, 0x00);
///
/// let file = File::open("/dev/kvm")?;
///
/// let version = unsafe { KVM_GET_API_VERSION.ioctl(&file)? };
/// println!("KVM API version: {version}");
/// # std::io::Result::Ok(())
/// ```
#[allow(non_snake_case)]
pub const fn _IO(ty: u8, nr: u8) -> Ioctl<NoArgs> {
    _IOC(_IOC_NONE, ty, nr, 0)
}

/// Creates an [`Ioctl`] that reads data of type `T` from the kernel.
///
/// A pointer to the data will be passed to `ioctl(2)`, and the kernel will fill the destination
/// with data.
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the *ioctl*
/// argument size limit.
/// This typically means that the wrong type `T` was specified.
///
/// # Examples
///
/// From `linux/random.h`:
///
/// ```c
/// /* ioctl()'s for the random number generator */
///
/// /* Get the entropy count. */
/// #define RNDGETENTCNT	_IOR( 'R', 0x00, int )
/// ```
///
/// ```
/// use std::fs::File;
/// use std::ffi::c_int;
/// use linux_ioctl::*;
///
/// const RNDGETENTCNT: Ioctl<*mut c_int> = _IOR(b'R', 0x00);
///
/// let file = File::open("/dev/urandom")?;
///
/// let mut entropy = 0;
/// unsafe { RNDGETENTCNT.ioctl(&file, &mut entropy)? };
///
/// println!("{entropy} bits of entropy in /dev/urandom");
/// # std::io::Result::Ok(())
/// ```
#[allow(non_snake_case)]
pub const fn _IOR<T>(ty: u8, nr: u8) -> Ioctl<*mut T> {
    const {
        assert!(size_of::<T>() <= (_IOC_SIZEMASK as usize));
    }
    _IOC(_IOC_READ, ty, nr, size_of::<T>()).with_arg()
}

/// Creates an [`Ioctl`] that writes data of type `T` to the kernel.
///
/// A pointer to the data will be passed to `ioctl(2)`, and the kernel will read the argument from
/// that location.
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the *ioctl*
/// argument size limit.
/// This typically means that the wrong type `T` was specified.
///
/// # Example
///
/// The *uinput* `ioctl` `UI_DEV_SETUP` can be invoked using [`_IOW`].
///
/// From `linux/uinput.h`:
///
/// ```c
/// /* ioctl */
/// #define UINPUT_IOCTL_BASE	'U'
/// #define UI_DEV_CREATE		_IO(UINPUT_IOCTL_BASE, 1)
/// #define UI_DEV_DESTROY		_IO(UINPUT_IOCTL_BASE, 2)
/// ...
/// #define UI_DEV_SETUP _IOW(UINPUT_IOCTL_BASE, 3, struct uinput_setup)
/// ```
///
/// ```rust
/// use std::{mem, fs::File, ffi::c_char};
/// use libc::uinput_setup;
/// use linux_ioctl::*;
///
/// const UINPUT_IOCTL_BASE: u8 = b'U';
/// const UI_DEV_CREATE: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 1);
/// const UI_DEV_DESTROY: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 2);
/// const UI_DEV_SETUP: Ioctl<*const uinput_setup> = _IOW(UINPUT_IOCTL_BASE, 3);
///
/// let uinput = File::options().write(true).open("/dev/uinput")?;
///
/// let mut setup: libc::uinput_setup = unsafe { mem::zeroed() };
/// setup.name[0] = b'A' as c_char; // (must not be blank)
/// unsafe {
///     UI_DEV_SETUP.ioctl(&uinput, &setup)?;
///     UI_DEV_CREATE.ioctl(&uinput)?;
///     // ...use the device...
///     UI_DEV_DESTROY.ioctl(&uinput)?;
/// }
/// # std::io::Result::Ok(())
/// ```
#[allow(non_snake_case)]
pub const fn _IOW<T>(ty: u8, nr: u8) -> Ioctl<*const T> {
    const {
        assert!(size_of::<T>() <= (_IOC_SIZEMASK as usize));
    }
    _IOC(_IOC_WRITE, ty, nr, size_of::<T>()).with_arg()
}

/// Creates an [`Ioctl`] that writes and reads data of type `T`.
///
/// A pointer to the data will be passed to `ioctl(2)`, and the kernel will read and write to the
/// data `T`.
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the *ioctl*
/// argument size limit.
/// This typically means that the wrong type `T` was specified.
#[allow(non_snake_case)]
pub const fn _IOWR<T>(ty: u8, nr: u8) -> Ioctl<*mut T> {
    const {
        assert!(size_of::<T>() <= (_IOC_SIZEMASK as usize));
    }
    _IOC(_IOC_READ | _IOC_WRITE, ty, nr, size_of::<T>()).with_arg()
}

/// Manually constructs an [`Ioctl`] from its components.
///
/// Also see [`Ioctl::from_raw`] for a way to interface with "legacy" ioctls that don't yet follow
/// this scheme.
///
/// Prefer to use [`_IO`], [`_IOR`], [`_IOW`], or [`_IOWR`] where possible.
///
/// # Arguments
///
/// - **`dir`**: *must* be one of [`_IOC_NONE`], [`_IOC_READ`], [`_IOC_WRITE`], or an ORed-together
///   combination. 0 is **not** valid on all architectures.
/// - **`ty`**: the `ioctl` group or type to identify the driver or subsystem. You can find a list
///   [here].
/// - **`nr`**: the `ioctl` number within its group.
/// - **`size`**: the size of the `ioctl`'s indirect argument. `ioctl`s that take an argument
///   directly (without passing a pointer to it) typically set this to 0.
///
/// [here]: https://www.kernel.org/doc/html/latest/userspace-api/ioctl/ioctl-number.html
///
/// # Panics
///
/// This function may panic when `dir` is not one of [`_IOC_NONE`], [`_IOC_READ`], [`_IOC_WRITE`],
/// or an ORed-together combination of those constants.
/// It may also panic when `size` exceeds the maximum parameter size.
///
/// # Example
///
/// `UI_GET_SYSNAME` is a polymorphic *ioctl* that can be invoked with a variety of buffer lengths.
/// This function can be used to bind to it.
///
/// From `linux/uinput.h`:
///
/// ```c
/// /* ioctl */
/// #define UINPUT_IOCTL_BASE	'U'
/// ...
/// #define UI_GET_SYSNAME(len)	_IOC(_IOC_READ, UINPUT_IOCTL_BASE, 44, len)
/// ```
///
/// ```no_run
/// use std::ffi::c_char;
/// use linux_ioctl::*;
///
/// const UINPUT_IOCTL_BASE: u8 = b'U';
/// const fn UI_GET_SYSNAME(len: usize) -> Ioctl<*mut c_char> {
///     _IOC(_IOC_READ, UINPUT_IOCTL_BASE, 44, len).with_arg()
/// }
///
/// // Use it like this:
/// unsafe {
/// #   let fd = &123;
///     let mut buffer = [0 as c_char; 16];
///     UI_GET_SYSNAME(16).ioctl(fd, buffer.as_mut_ptr())?;
/// }
/// # std::io::Result::Ok(())
/// ```
#[allow(non_snake_case)]
#[inline]
pub const fn _IOC(dir: u32, ty: u8, nr: u8, size: usize) -> Ioctl<NoArgs> {
    use consts::*;

    assert!(
        dir & !(_IOC_NONE | _IOC_WRITE | _IOC_READ) == 0,
        "`dir` must be a combination of `_IOC_NONE`, `_IOC_READ`, and `_IOC_WRITE`"
    );
    assert!(size <= (_IOC_SIZEMASK as usize));

    let request = (dir << _IOC_DIRSHIFT)
        | ((ty as u32) << _IOC_TYPESHIFT)
        | ((nr as u32) << _IOC_NRSHIFT)
        | ((size as u32) << _IOC_SIZESHIFT);
    Ioctl::from_raw(request)
}
