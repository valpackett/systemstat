//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

#[cfg(not(windows))] extern crate libc;
#[cfg(windows)] extern crate winapi;
#[cfg(windows)] extern crate kernel32;
extern crate bytesize;
extern crate time;
extern crate chrono;
#[macro_use] extern crate lazy_static;

pub mod platform;
pub mod data;

pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
pub use self::data::*;
