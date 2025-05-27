# `linux-ioctl`

Simple porting tools for Linux-style `ioctl(2)` driver interfaces.

## Motivation

I mainly wrote this crate because I was unhappy with the alternatives:

- `libc::ioctl`
  - The raw `ioctl` function provides no built-in error conversion and requires knowing the `ioctl` request number.
  - `libc` does provide `const fn`s that mirror the `_IOx` macros, but they are only exposed on Linux and have no type safety.
- `nix::ioctl_X!`
  - Looks nothing like the `ioctl` definitions found in C headers.
  - Requires very frequent conscious `nix` updates because it publishes *A LOT* of breaking changes.
- Other crates for this are either abandoned or also frequently publish breaking changes that I'd have to consciously migrate to.
  - Instead, I want a crate that tries to *only* cover `ioctl`s, and has a decently designed API that doesn't see much breakage.
- Almost none of these alternatives except `libc` make translation of C headers to Rust very easy.
  - The Rust code required to do the thing often looks completely different to the equivalent C definitions.
  - As `ioctl`s are a low-level interface often accessed by directly porting a C header, it is beneficial if the result is easy to visually compare to the original C code.

Thus, this library was born in an attempt to address these problems.

## Example

Let's use `uinput` to create a userspace input device.

From `linux/uinput.h`:

```c
/* ioctl */
#define UINPUT_IOCTL_BASE	'U'
#define UI_DEV_CREATE		_IO(UINPUT_IOCTL_BASE, 1)
#define UI_DEV_DESTROY		_IO(UINPUT_IOCTL_BASE, 2)
...
#define UI_DEV_SETUP _IOW(UINPUT_IOCTL_BASE, 3, struct uinput_setup)
```

```rust
use std::{mem, fs::File, ffi::c_char};
use libc::uinput_setup;
use linux_ioctl::{Ioctl, _IO, _IOW};

const UINPUT_IOCTL_BASE: u8 = b'U';
const UI_DEV_CREATE: Ioctl  = _IO(UINPUT_IOCTL_BASE, 1);
const UI_DEV_DESTROY: Ioctl = _IO(UINPUT_IOCTL_BASE, 2);
const UI_DEV_SETUP: Ioctl<*const uinput_setup> = _IOW(UINPUT_IOCTL_BASE, 3);

let uinput = File::options().write(true).open("/dev/uinput")?;

let mut setup: libc::uinput_setup = unsafe { mem::zeroed() };
setup.name[0] = b'A' as c_char; // (must not be blank)
unsafe {
    UI_DEV_SETUP.ioctl(&uinput, &setup)?;
    UI_DEV_CREATE.ioctl(&uinput)?;
    // ...use the device...
    UI_DEV_DESTROY.ioctl(&uinput)?;
}
std::io::Result::Ok(())
```

## Rust Support

This library targets the latest Rust version.

Older Rust versions are supported by equally older versions of this crate. For example, to use a
version of Rust that was succeeded 6 months ago, you'd also use an at least 6 month old version of
this library.

Compatibility with older Rust versions may be provided on a best-effort basis.
