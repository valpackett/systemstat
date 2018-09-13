//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

#[cfg(windows)]
extern crate winapi;
extern crate libc;
extern crate time;
extern crate chrono;
extern crate bytesize;
#[macro_use]
extern crate lazy_static;
#[cfg(any(target_os = "linux", target_os = "android"))]
#[macro_use]
extern crate nom;

pub mod platform;
pub mod data;

pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
pub use self::data::*;
