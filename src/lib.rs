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
//! use uoctl::*;
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
//! Despite being about Linux APIs, and following the Linux convention for declaring `ioctl` codes,
//! this library should also work on other operating systems that implement a Linux-comparible
//! `ioctl`-based API.
//!
//! For example, FreeBSD implements a variety of compatible interfaces like *evdev* and *V4L2*.
//!
//! # Safety
//!
//! To safely perform an `ioctl`, the actual behavior of the kernel-side has to match the behavior
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
//! several drivers and subsystems share the same `ioctl` "type" value, which may lead to an ioctl
//! request code that is interpreted differently, depending on which driver receives the request.
//! Since the `ioctl` request code encodes the size of the argument type, this operation is unlikely
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

#[cfg(any(target_os = "linux", target_os = "android"))]
#[path = "platform/linux.rs"]
mod platform;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos",
    target_os = "watchos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
))]
#[path = "platform/bsd.rs"]
mod platform;

use std::{ffi::c_int, fmt, io, marker::PhantomData, ops::BitOr, os::fd::AsRawFd};

/// An `ioctl`.
///
/// [`Ioctl`] can represent `ioctl`s that take either no arguments or a single argument.
/// If `T` is [`NoArgs`], the `ioctl` takes no arguments.
/// For other values of `T`, the `ioctl` takes `T` as its only argument.
/// Often, the argument `T` is a pointer to a struct that contains the actual arguments.
///
/// While [`Ioctl`] cannot handle `ioctl`s that require passing more than one argument to the
/// `ioctl(2)` function, Linux doesn't have any `ioctl`s that take more than one argument, and is
/// unlikely to gain any in the future.
///
/// The [`Ioctl`] type is constructed with the free functions [`_IO`], [`_IOR`], [`_IOW`],
/// [`_IOWR`], and [`_IOC`].
/// For legacy `ioctl`s, it can also be created via [`Ioctl::from_raw`].
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
    /// This can be used for legacy `ioctl`s that were defined before the `_IOx` macros were
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
    /// use std::io;
    /// use std::fs::File;
    /// use std::ffi::c_int;
    /// use uoctl::*;
    ///
    /// const FIONREAD: Ioctl<*mut c_int> = Ioctl::from_raw(0x541B);
    ///
    /// let file = File::open("/dev/ptmx")
    ///     .map_err(|e| io::Error::new(e.kind(), format!("failed to open `/dev/ptmx`: {e}")))?;
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

    /// Changes the `ioctl` argument type to `T2`.
    ///
    /// This can be used for `ioctl`s that incorrectly declare their type, or for `ioctl`s that take
    /// a by-value argument, rather than [`_IOW`]-type `ioctl`s that take their argument indirectly
    /// through a pointer.
    ///
    /// Returns an [`Ioctl`] that passes an argument of type `T2` to the kernel, while using the
    /// `ioctl` request code from `self`.
    ///
    /// # Examples
    ///
    /// The `KVM_CREATE_VM` `ioctl` is declared with [`_IO`], but expects an `int` argument to be
    /// passed to `ioctl(2)`, specifying the VM type (`KVM_VM_*`).
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
    /// use uoctl::*;
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

    /// Returns the `ioctl` request code.
    ///
    /// This is passed to `ioctl(2)` as its second argument.
    ///
    /// Note that the second argument of `ioctl(2)` may be `int` or `unsigned long`, depending on
    /// target platform. [`Ioctl::ioctl`] will convert the type as needed, but user code that uses
    /// [`Ioctl::request`] may have to do it manually.
    ///
    /// This library always uses [`u32`] in its interface because [`u32`] is the smallest
    /// platform-independent type capable of encoding every `ioctl` number used in Linux' encoding
    /// scheme.
    pub const fn request(self) -> u32 {
        self.request
    }
}

impl<T> Ioctl<*const T> {
    /// Changes the [`Ioctl`] argument type to be passed directly instead of behind a pointer.
    ///
    /// Does not change the request code.
    ///
    /// Many linux headers define `ioctl`s like `_IOW('U', 100, int)`, but then expect the `int`
    /// argument to be passed as a direct argument to `ioctl(2)` instead of passing a pointer.
    /// This method can be used to bind to these `ioctl`s.
    ///
    /// # Example
    ///
    /// `uinput` defines several `ioctl`s where this method is useful:
    ///
    /// ```c
    /// #define UI_SET_EVBIT		_IOW(UINPUT_IOCTL_BASE, 100, int)
    /// ```
    ///
    /// ```
    /// use std::ffi::c_int;
    /// use uoctl::{Ioctl, _IOW};
    ///
    /// const UI_SET_EVBIT: Ioctl<c_int> = _IOW(b'U', 100).with_direct_arg();
    /// ```
    #[inline]
    pub const fn with_direct_arg(self) -> Ioctl<T> {
        self.with_arg()
    }

    /// Casts the [`Ioctl`] so that it takes a `*mut` pointer instead of a `*const` pointer.
    ///
    /// This can be used to fix an `ioctl` that is incorrectly declared as only *reading* its
    /// argument through the pointer (with [`_IOW`]), when in reality the kernel can *write* through
    /// the pointer as well.
    ///
    /// An ioctl that writes through its pointer argument, but is incorrectly declared as
    /// [`Ioctl<*const T>`] will generally cause UB when invoked with an immutable reference.
    ///
    /// Also see [`Ioctl::cast_const`] for the opposite direction.
    ///
    /// # Example
    ///
    /// The `EVIOCSFF` ioctl from evdev is incorrectly declared with [`_IOW`], but may write to the
    /// data, as documented here:
    ///
    /// > “request” must be EVIOCSFF.
    /// >
    /// > “effect” points to a structure describing the effect to upload. The effect is uploaded, but
    /// > not played. **The content of effect may be modified.**
    ///
    /// <https://www.kernel.org/doc/html/latest/input/ff.html>
    ///
    /// ```
    /// use std::ffi::c_void as ff_effect;
    /// use uoctl::{Ioctl, _IOW};
    ///
    /// pub const EVIOCSFF: Ioctl<*mut ff_effect> = _IOW(b'E', 0x80).cast_mut();
    /// ```
    #[inline]
    pub const fn cast_mut(self) -> Ioctl<*mut T> {
        self.with_arg()
    }
}

impl<T> Ioctl<*mut T> {
    /// Casts the [`Ioctl`] so that it takes a `*const` pointer instead of a `*mut` pointer.
    ///
    /// This performs the opposite operation of [`Ioctl::cast_mut`], and can be used when an `ioctl`
    /// is incorrectly declared as writing to its argument (yielding an [`Ioctl<*mut T>`]) when it
    /// actually only reads from it.
    ///
    /// Only use this method if you are sure it is correct! If the `ioctl` *does* write through the
    /// pointer, the result is likely UB!
    #[inline]
    pub const fn cast_const(self) -> Ioctl<*const T> {
        self.with_arg()
    }
}

impl Ioctl<NoArgs> {
    /// Performs an `ioctl` that doesn't take an argument.
    ///
    /// On success, returns the value returned by the `ioctl` syscall. On error (when `ioctl`
    /// returns -1), returns the error from *errno*.
    ///
    /// Note that the actual `ioctl(2)` call performed will pass 0 as a dummy argument to the
    /// `ioctl`. This is because some Linux `ioctl`s are declared without an argument, but will fail
    /// unless they receive 0 as their argument (eg. `KVM_GET_API_VERSION`). There should be no harm
    /// in passing this argument unconditionally, as the kernel will typically just ignore excess
    /// arguments.
    ///
    /// # Safety
    ///
    /// This method performs an arbitrary `ioctl` on an arbitrary file descriptor.
    /// The caller has to ensure that any safety requirements of the `ioctl` are met, that `T`
    /// denotes the correct argument type, and that `fd` is valid (open) and belongs to the driver
    /// it expects.
    pub unsafe fn ioctl(self, fd: &impl AsRawFd) -> io::Result<c_int> {
        let res = unsafe { libc::ioctl(fd.as_raw_fd(), self.request as _, 0) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }
}

impl<T> Ioctl<T> {
    /// Performs an `ioctl` that takes an argument of type `T`.
    ///
    /// Returns the value returned by the `ioctl(2)` invocation, or an I/O error if the call failed.
    ///
    /// For many `ioctl`s, `T` will be a pointer to the actual argument.
    /// The caller must ensure that it points to valid data that conforms to the requirements of the
    /// `ioctl`.
    ///
    /// # Safety
    ///
    /// This method performs an arbitrary `ioctl` on an arbitrary file descriptor.
    /// The caller has to ensure that any safety requirements of the `ioctl` are met, that `T`
    /// denotes the correct argument type, and that `fd` is valid (open) and belongs to the driver
    /// it expects.
    pub unsafe fn ioctl(self, fd: &impl AsRawFd, arg: T) -> io::Result<c_int> {
        let res = unsafe { libc::ioctl(fd.as_raw_fd(), self.request as _, arg) };
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
/// use uoctl::*;
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

/// Direction of an [`Ioctl`].
///
/// Used by [`_IOC`]. Constructed by using the constants [`_IOC_NONE`], [`_IOC_READ`], and
/// [`_IOC_WRITE`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Dir(u32);

impl BitOr for Dir {
    type Output = Dir;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        // `_IOC_NONE` is 0 on x86, but non-zero on other architectures. It is invalid and
        // non-portable to combine it with other usages, so we prevent it here.
        // This check will easily optimize out in almost all cases, since the direction is nearly
        // always a compile-time constant.
        if (self == _IOC_NONE && rhs != _IOC_NONE) || (self != _IOC_NONE && rhs == _IOC_NONE) {
            panic!("`_IOC_NONE` cannot be combined with other values");
        }

        Self(self.0 | rhs.0)
    }
}

impl fmt::Debug for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == _IOC_READ | _IOC_WRITE {
            f.write_str("_IOC_READ | _IOC_WRITE")
        } else if *self == _IOC_READ {
            f.write_str("_IOC_READ")
        } else if *self == _IOC_WRITE {
            f.write_str("_IOC_WRITE")
        } else if *self == _IOC_NONE {
            f.write_str("_IOC_NONE")
        } else {
            write!(f, "{:#x}", self.0)
        }
    }
}

/// Indicates that an `ioctl` neither reads nor writes data through its argument.
///
/// Identical to [`IOC_VOID`]. [`_IOC_NONE`] is a Linuxism, while [`IOC_VOID`] is used by other
/// systems.
pub const _IOC_NONE: Dir = Dir(platform::_IOC_NONE);

/// Indicates that an `ioctl` reads data from the kernel through its pointer argument.
///
/// Identical to [`IOC_OUT`]. [`_IOC_READ`] is a Linuxism, while [`IOC_OUT`] is used by other
/// systems.
pub const _IOC_READ: Dir = Dir(platform::_IOC_READ);

/// Indicates that an `ioctl` writes data to the kernel through its pointer argument.
///
/// Identical to [`IOC_IN`]. [`_IOC_WRITE`] is a Linuxism, while [`IOC_IN`] is used by other
/// systems.
pub const _IOC_WRITE: Dir = Dir(platform::_IOC_WRITE);

/// Indicates that an `ioctl` both reads and writes data through its pointer argument.
///
/// Equivalent to `_IOC_READ | _IOC_WRITE`, which doesn't work in `const` contexts.
///
/// C code always uses `_IOC_READ | _IOC_WRITE` instead of a dedicated constant.
pub const _IOC_READ_WRITE: Dir = Dir(platform::_IOC_READ | platform::_IOC_WRITE);

/// Indicates that an `ioctl` neither reads nor writes data through its argument.
///
/// Identical to [`_IOC_NONE`].
pub const IOC_VOID: Dir = _IOC_NONE;

/// Indicates that an `ioctl` reads data from the kernel through its pointer argument.
///
/// Identical to [`_IOC_READ`].
pub const IOC_OUT: Dir = _IOC_READ;

/// Indicates that an `ioctl` writes data to the kernel through its pointer argument.
///
/// Identical to [`_IOC_WRITE`].
pub const IOC_IN: Dir = _IOC_WRITE;

/// Indicates that an `ioctl` both reads and writes data through its pointer argument.
///
/// Identical to [`_IOC_READ_WRITE`] and `_IOC_READ | _IOC_WRITE`.
pub const IOC_INOUT: Dir = _IOC_READ_WRITE;

/// Creates an [`Ioctl`] that doesn't read or write any userspace data.
///
/// This type of ioctl can return an `int` to userspace via the return value of the `ioctl` syscall.
/// By default, the returned [`Ioctl`] takes no argument.
/// [`Ioctl::with_arg`] can be used to pass a direct argument to the `ioctl`.
///
/// # Example
///
/// `KVM_GET_API_VERSION` is an `ioctl` that does not take any arguments. The API version is
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
/// use uoctl::*;
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
/// By default, a pointer to the data will be passed to `ioctl(2)`, and the kernel will fill the
/// destination with data.
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the `ioctl`
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
/// use uoctl::*;
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
        assert!(size_of::<T>() <= platform::MAX_ARG_SIZE);
    }
    _IOC(_IOC_READ, ty, nr, size_of::<T>())
}

/// Creates an [`Ioctl`] that writes data of type `T` to the kernel.
///
/// By default, a pointer to the data will be passed to `ioctl(2)`, and the kernel will read the
/// argument from that location.
/// This is generally correct if the argument is a `struct`, but if the argument is a primitive type
/// like `int`, or is already a pointer like `char*`, many drivers expect the argument to be passed
/// to `ioctl(2)` *without* indirection.
/// To bind to those `ioctl`s, you can call [`Ioctl::with_direct_arg`] on the [`Ioctl`] returned by
/// [`_IOW`].
///
/// **Note**: Some Linux `ioctl`s are **incorrectly** declared with [`_IOW`].
/// They *will* write to the argument, and if the pointer you pass doesn't have write permission it
/// *will* cause UB.
/// Use [`Ioctl::cast_mut`] before invoking the [`Ioctl`] to fix these kinds of misdeclared
/// `ioctl`s!
///
/// Apart from reading comments in the header files, and looking at the Linux source code, there is
/// no reliable way of finding out which `ioctl` definitions are wrong like that.
/// Good luck!
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the `ioctl`
/// argument size limit.
/// This typically means that the wrong type `T` was specified.
///
/// # Example
///
/// Let's create a virtual input device with *uinput*.
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
/// ...
/// #define UI_SET_EVBIT		_IOW(UINPUT_IOCTL_BASE, 100, int)
/// #define UI_SET_KEYBIT		_IOW(UINPUT_IOCTL_BASE, 101, int)
/// ```
///
/// From `linux/input.h`:
///
/// ```c
/// #define EV_KEY			0x01
/// ...
/// #define KEY_A			30
/// ```
///
/// ```rust
/// use std::{mem, fs::File, ffi::{c_char, c_int}};
/// use libc::uinput_setup;
/// use uoctl::*;
///
/// const UINPUT_IOCTL_BASE: u8 = b'U';
/// const UI_DEV_CREATE: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 1);
/// const UI_DEV_DESTROY: Ioctl<NoArgs> = _IO(UINPUT_IOCTL_BASE, 2);
/// const UI_DEV_SETUP: Ioctl<*const uinput_setup> = _IOW(UINPUT_IOCTL_BASE, 3);
/// // These two expect their argument to be passed directly instead of behind a pointer:
/// const UI_SET_EVBIT: Ioctl<c_int> = _IOW(UINPUT_IOCTL_BASE, 100).with_direct_arg();
/// const UI_SET_KEYBIT: Ioctl<c_int> = _IOW(UINPUT_IOCTL_BASE, 101).with_direct_arg();
///
/// const EV_KEY: c_int = 0x01;
/// const KEY_A: c_int = 30;
///
/// let uinput = File::options().write(true).open("/dev/uinput")?;
///
/// // Enable the "A" key:
/// unsafe {
///     UI_SET_EVBIT.ioctl(&uinput, EV_KEY)?;
///     UI_SET_KEYBIT.ioctl(&uinput, KEY_A)?;
/// }
///
/// let mut setup: uinput_setup = unsafe { mem::zeroed() };
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
        assert!(size_of::<T>() <= platform::MAX_ARG_SIZE);
    }
    _IOC(_IOC_WRITE, ty, nr, size_of::<T>())
}

/// Creates an [`Ioctl`] that writes and reads data of type `T`.
///
/// By default, a pointer to the data will be passed to `ioctl(2)`, and the kernel will read and
/// write to the data `T`.
///
/// # Errors
///
/// This method will cause a compile-time assertion failure if the size of `T` exceeds the `ioctl`
/// argument size limit.
/// This typically means that the wrong type `T` was specified.
#[allow(non_snake_case)]
pub const fn _IOWR<T>(ty: u8, nr: u8) -> Ioctl<*mut T> {
    const {
        assert!(size_of::<T>() <= platform::MAX_ARG_SIZE);
    }
    _IOC(_IOC_READ_WRITE, ty, nr, size_of::<T>())
}

/// Creates an [`Ioctl`] that writes an `int` to the kernel.
///
/// This is a BSD-specific function that only exists in the BSD C headers. Using it on other systems
/// may not result in the correct `ioctl` request code.
///
/// Linux does not have a function/macro like this, and it typically uses [`_IOW`] to define
/// `ioctl`s that pass `int`s (often necessitating a call to [`Ioctl::with_direct_arg`]).
#[allow(non_snake_case)]
pub const fn _IOWINT(group: u8, nr: u8) -> Ioctl<c_int> {
    _IOC(IOC_VOID, group, nr, size_of::<c_int>())
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
/// - **`dir`**: Direction of the ioctl. One of [`_IOC_NONE`], [`_IOC_READ`], [`_IOC_WRITE`], or
///   `_IOC_READ | _IOC_WRITE` (aka [`_IOC_READ_WRITE`]).
/// - **`ty`**: the `ioctl` group or type to identify the driver or subsystem. You can find a list
///   [here].
/// - **`nr`**: the `ioctl` number within its group.
/// - **`size`**: the size of the `ioctl`'s (direct or indirect) argument.
///
/// [here]: https://www.kernel.org/doc/html/latest/userspace-api/ioctl/ioctl-number.html
///
/// # Panics
///
/// This function may panic when `size` exceeds the (platform-specific) maximum parameter size.
///
/// # Example
///
/// `UI_GET_SYSNAME` is a polymorphic `ioctl` that can be invoked with a variety of buffer lengths.
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
/// use uoctl::*;
///
/// const UINPUT_IOCTL_BASE: u8 = b'U';
/// const fn UI_GET_SYSNAME(len: usize) -> Ioctl<*mut c_char> {
///     _IOC(_IOC_READ, UINPUT_IOCTL_BASE, 44, len)
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
pub const fn _IOC<T: ?Sized>(dir: Dir, ty: u8, nr: u8, size: usize) -> Ioctl<T> {
    assert!(size <= platform::MAX_ARG_SIZE);

    let request = platform::_IOC(dir.0, ty as u32, nr as u32, size as u32);
    Ioctl::from_raw(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_or() {
        assert_ne!(_IOC_NONE, _IOC_READ);
        assert_ne!(_IOC_NONE, _IOC_WRITE);

        assert_eq!(_IOC_READ | _IOC_WRITE, _IOC_READ_WRITE);
        assert_eq!(_IOC_READ | _IOC_READ, _IOC_READ);
        assert_eq!(_IOC_WRITE | _IOC_WRITE, _IOC_WRITE);
        assert_eq!(_IOC_NONE | _IOC_NONE, _IOC_NONE);
    }

    #[test]
    #[should_panic(expected = "`_IOC_NONE` cannot be combined with other values")]
    fn dir_none_or_read() {
        let _ = _IOC_NONE | _IOC_READ;
    }

    #[test]
    #[should_panic(expected = "`_IOC_NONE` cannot be combined with other values")]
    fn dir_none_or_write() {
        let _ = _IOC_NONE | _IOC_WRITE;
    }

    #[test]
    #[should_panic(expected = "`_IOC_NONE` cannot be combined with other values")]
    fn dir_read_or_none() {
        let _ = _IOC_READ | _IOC_NONE;
    }

    #[test]
    #[should_panic(expected = "`_IOC_NONE` cannot be combined with other values")]
    fn dir_write_or_none() {
        let _ = _IOC_WRITE | _IOC_NONE;
    }
}
