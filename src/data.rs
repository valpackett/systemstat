//! This module provides the data structures that represent system information.
//!
//! They're always the same across all platforms.

use std::io;
pub use std::time::Duration;
pub use std::net::{Ipv4Addr, Ipv6Addr};
pub use std::collections::BTreeMap;
pub use chrono::{DateTime, UTC, NaiveDateTime};
pub use bytesize::ByteSize;

/// A wrapper for a measurement that takes time.
///
/// Time should pass between getting the object and calling .done() on it.
pub struct DelayedMeasurement<T> {
    res: Box<Fn() -> io::Result<T>>
}

impl<T> DelayedMeasurement<T> {
    #[inline(always)]
    pub fn new(f: Box<Fn() -> io::Result<T>>) -> DelayedMeasurement<T> {
        DelayedMeasurement { res: f }
    }

    #[inline(always)]
    pub fn done(&self) -> io::Result<T> {
        (self.res)()
    }
}

#[derive(Debug, Clone)]
pub struct CPULoad {
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub interrupt: f32,
    pub idle: f32,
}

impl CPULoad {
    #[inline(always)]
    pub fn avg_add(self, rhs: &Self) -> Self {
        CPULoad {
            user: (self.user + rhs.user) / 2.0,
            nice: (self.nice + rhs.nice) / 2.0,
            system: (self.system + rhs.system) / 2.0,
            interrupt: (self.interrupt + rhs.interrupt) / 2.0,
            idle: (self.idle + rhs.idle) / 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadAverage {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

#[cfg(target_os = "freebsd")]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub active: ByteSize,
    pub inactive: ByteSize,
    pub wired: ByteSize,
    pub cache: ByteSize,
    pub zfs_arc: ByteSize,
    pub free: ByteSize,
}

#[cfg(target_os = "openbsd")]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub active: ByteSize,
    pub inactive: ByteSize,
    pub wired: ByteSize,
    pub cache: ByteSize,
    pub free: ByteSize,
    pub paging: ByteSize,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub meminfo: BTreeMap<String, ByteSize>,
}

#[derive(Debug, Clone)]
pub struct Memory {
    pub total: ByteSize,
    pub free: ByteSize,
    pub platform_memory: PlatformMemory,
}

#[derive(Debug, Clone)]
pub struct BatteryLife {
    pub remaining_capacity: f32,
    pub remaining_time: Duration,
}

#[derive(Debug, Clone)]
pub struct Filesystem {
    pub files: usize,
    pub free: ByteSize,
    pub avail: ByteSize,
    pub total: ByteSize,
    pub name_max: usize,
    pub fs_type: String,
    pub fs_mounted_from: String,
    pub fs_mounted_on: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpAddr {
    Empty,
    Unsupported,
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

#[derive(Debug, Clone)]
pub struct NetworkAddrs {
    pub addr: IpAddr,
    pub netmask: IpAddr,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
    pub addrs: Vec<NetworkAddrs>,
}
