//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

extern crate bytesize;
extern crate chrono;
#[cfg_attr(
    any(target_os = "freebsd", target_os = "openbsd", target_os = "macos"),
    macro_use
)]
extern crate lazy_static;
extern crate libc;
#[cfg(any(target_os = "linux", target_os = "android"))]
extern crate nom;
#[cfg(feature = "serde")]
extern crate serde;
#[cfg(windows)]
extern crate winapi;

pub mod data;
pub mod platform;

pub use self::data::*;
pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
