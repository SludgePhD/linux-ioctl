# Changelog

## v0.2.3

- Add support for FreeBSD and XNU (macOS, tvOS, and iOS).
- Add BSD-style ioctl direction aliases (`IOC_VOID`, `IOC_IN`, etc).
- Make `Ioctl::request` a `const fn`.
- Relicense under the 0-clause BSD license.

## v0.2.2

- Add `Ioctl::cast_mut` and `Ioctl::cast_const`, mirroring the methods on raw pointers.

## v0.2.1

- Add `Ioctl::with_direct_arg` for more convenient binding to `_IOW` `ioctl`s that take a direct argument.
- Minor documentation improvements.

## v0.2.0

- Make `_IOC` generic over the ioctl argument type.
  - This makes `_IOC` easier to use by no longer requiring virtually every use of it to be followed-up with `.with_arg()`.

- `_IOC_x` constants are now newtypes.
  - This makes it harder to pass invalid values to `_IOC`.
  - Specifically, `_IOC_NONE` is 0 on x86, but non-zero on other architectures, posing a portability hazard if 0 is passed as a literal by mistake.

## v0.1.0

Initial release.
