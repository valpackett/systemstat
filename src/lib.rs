//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

extern crate bytesize;
extern crate chrono;
extern crate libc;
extern crate time;
#[cfg(windows)]
extern crate winapi;
#[macro_use]
extern crate lazy_static;
#[cfg(any(target_os = "linux", target_os = "android"))]
#[macro_use]
extern crate nom;

pub mod data;
pub mod platform;

pub use self::data::*;
pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
