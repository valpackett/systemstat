//! This library provides a way to access system information such as CPU load, mounted filesystems,
//! network interfaces, etc.

#[cfg_attr(
    any(target_os = "freebsd", target_os = "openbsd", target_os = "macos"),
    macro_use
)]
extern crate lazy_static;

#[cfg(feature = "serde")]
extern crate the_serde as serde;

pub mod data;
pub mod platform;

pub use self::data::*;
pub use self::platform::Platform;
pub use self::platform::PlatformImpl as System;
