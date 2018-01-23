//! This module provides the data structures that represent system information.
//!
//! They're always the same across all platforms.

use std::io;
pub use std::time::Duration;
pub use std::net::{Ipv4Addr, Ipv6Addr};
pub use std::collections::BTreeMap;
pub use chrono::{DateTime, Utc, NaiveDateTime};
pub use bytesize::ByteSize;
use std::ops::Sub;

/// A wrapper for a measurement that takes time.
///
/// Time should pass between getting the object and calling .done() on it.
pub struct DelayedMeasurement<T> {
    res: Box<Fn() -> io::Result<T>>,
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

#[derive(Debug, Clone, Copy)]
pub struct CpuTime {
    pub user: usize,
    pub nice: usize,
    pub system: usize,
    pub interrupt: usize,
    pub idle: usize,
    pub other: usize,
}

impl<'a> Sub<&'a CpuTime> for CpuTime {
    type Output = CpuTime;

    #[inline(always)]
    fn sub(self, rhs: &CpuTime) -> CpuTime {
        CpuTime {
            user: self.user.saturating_sub(rhs.user),
            nice: self.nice.saturating_sub(rhs.nice),
            system: self.system.saturating_sub(rhs.system),
            interrupt: self.interrupt.saturating_sub(rhs.interrupt),
            idle: self.idle.saturating_sub(rhs.idle),
            other: self.other.saturating_sub(rhs.other),
        }
    }
}

impl CpuTime {
    pub fn to_cpuload(&self) -> CPULoad {
        let total = self.user + self.nice + self.system + self.interrupt + self.idle + self.other;
        if total == 0 {
            CPULoad {
                user: 0.0,
                nice: 0.0,
                system: 0.0,
                interrupt: 0.0,
                idle: 0.0,
            }
        } else {
            CPULoad {
                user: self.user as f32 / total as f32,
                nice: self.nice as f32 / total as f32,
                system: self.system as f32 / total as f32,
                interrupt: self.interrupt as f32 / total as f32,
                idle: self.idle as f32 / total as f32,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadAverage {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub load: u32,
    pub total_phys: ByteSize,
    pub avail_phys: ByteSize,
    pub total_pagefile: ByteSize,
    pub avail_pagefile: ByteSize,
    pub total_virt: ByteSize,
    pub avail_virt: ByteSize,
    pub avail_ext: ByteSize,
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

pub type Disk = String;

#[derive(Debug, Clone)]
pub struct BlockDeviceStats {
    pub name: String,
    pub read_ios: u64,
    pub read_merges: u64,
    pub read_sectors: u64,
    pub read_ticks: u64,
    pub write_ios: u64,
    pub write_merges: u64,
    pub write_sectors: u64,
    pub write_ticks: u64,
    pub in_flight: u64,
    pub io_ticks: u64,
    pub time_in_queue: u64
}
