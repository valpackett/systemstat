//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

#[cfg(windows)]
extern crate winapi;
extern crate libc;
extern crate chrono;
extern crate bytesize;
#[cfg_attr(any(target_os = "freebsd", target_os = "openbsd", target_os = "macos"), macro_use)]
extern crate lazy_static;
#[cfg(any(target_os = "linux", target_os = "android"))]
extern crate nom;
#[cfg(feature = "serde")]
extern crate serde;

pub mod platform;
pub mod data;

pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
pub use self::data::*;
