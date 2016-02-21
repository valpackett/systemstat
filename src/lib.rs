extern crate libc;
#[macro_use] extern crate lazy_static;

pub mod platform;
pub mod data;

pub use self::platform::PlatformImpl as System;
pub use self::data::*;
