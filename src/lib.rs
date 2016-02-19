extern crate libc;

pub mod platform;
pub mod data;

pub use self::platform::PlatformImpl as System;
pub use self::data::*;
