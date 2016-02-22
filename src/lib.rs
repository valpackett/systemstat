//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

extern crate libc;
extern crate bytesize;
#[macro_use] extern crate lazy_static;

pub mod platform;
pub mod data;

pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
pub use self::data::*;
