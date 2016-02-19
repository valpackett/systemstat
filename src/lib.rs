extern crate libc;

pub mod platform;
pub mod data;

pub use self::platform::*;
pub use self::data::*;

pub static SYSTEM: PlatformImpl = PlatformImpl;
