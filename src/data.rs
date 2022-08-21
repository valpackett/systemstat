//! This module provides the data structures that represent system information.
//!
//! They're always the same across all platforms.

pub use bytesize::ByteSize;
pub use std::collections::BTreeMap;
use std::io;
pub use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::Sub;
pub use std::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[inline(always)]
pub fn saturating_sub_bytes(l: ByteSize, r: ByteSize) -> ByteSize {
    ByteSize::b(l.as_u64().saturating_sub(r.as_u64()))
}

/// A wrapper for a measurement that takes time.
///
/// Time should pass between getting the object and calling .done() on it.
pub struct DelayedMeasurement<T> {
    res: Box<dyn Fn() -> io::Result<T> + Send>,
}

impl<T> DelayedMeasurement<T> {
    #[inline(always)]
    pub fn new(f: Box<dyn Fn() -> io::Result<T> + Send>) -> DelayedMeasurement<T> {
        DelayedMeasurement { res: f }
    }

    #[inline(always)]
    pub fn done(&self) -> io::Result<T> {
        (self.res)()
    }
}

#[cfg(not(target_os = "linux"))]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformCpuLoad {}

#[cfg(target_os = "linux")]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformCpuLoad {
    pub iowait: f32,
}

impl PlatformCpuLoad {
    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub fn avg_add(self, rhs: &Self) -> Self {
        PlatformCpuLoad {
            iowait: (self.iowait + rhs.iowait) / 2.0,
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    pub fn avg_add(self, _rhs: &Self) -> Self {
        PlatformCpuLoad {}
    }

    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub fn zero() -> Self {
        PlatformCpuLoad { iowait: 0.0 }
    }

    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    pub fn zero() -> Self {
        PlatformCpuLoad {}
    }

    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub fn from(input: f32) -> Self {
        PlatformCpuLoad { iowait: input }
    }

    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    pub fn from(_input: f32) -> Self {
        PlatformCpuLoad {}
    }
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct CPULoad {
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub interrupt: f32,
    pub idle: f32,
    pub platform: PlatformCpuLoad,
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
            platform: self.platform.avg_add(&rhs.platform),
        }
    }
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
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
                platform: PlatformCpuLoad::zero(),
            }
        } else {
            CPULoad {
                user: self.user as f32 / total as f32,
                nice: self.nice as f32 / total as f32,
                system: self.system as f32 / total as f32,
                interrupt: self.interrupt as f32 / total as f32,
                idle: self.idle as f32 / total as f32,
                platform: PlatformCpuLoad::from(self.other as f32 / total as f32),
            }
        }
    }
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct LoadAverage {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

#[cfg(target_os = "windows")]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
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
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
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
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub total: ByteSize,
    pub active: ByteSize,
    pub inactive: ByteSize,
    pub wired: ByteSize,
    pub cache: ByteSize,
    pub free: ByteSize,
    pub paging: ByteSize,
    pub sw: ByteSize,
    pub swinuse: ByteSize,
    pub swonly: ByteSize,
}

#[cfg(target_os = "netbsd")]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub pageshift: i64,
    pub total: ByteSize,
    pub active: ByteSize,
    pub inactive: ByteSize,
    pub wired: ByteSize,
    pub free: ByteSize,
    pub paging: ByteSize,
    pub anon: ByteSize,
    pub files: ByteSize,
    pub exec: ByteSize,
    pub sw: ByteSize,
    pub swinuse: ByteSize,
    pub swonly: ByteSize,
}

#[cfg(target_os = "macos")]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub total: ByteSize,
    pub active: ByteSize,
    pub inactive: ByteSize,
    pub wired: ByteSize,
    pub free: ByteSize,
    pub purgeable: ByteSize,
    pub speculative: ByteSize,
    pub compressor: ByteSize,
    pub throttled: ByteSize,
    pub external: ByteSize,
    pub internal: ByteSize,
    pub uncompressed_in_compressor: ByteSize,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformMemory {
    pub meminfo: BTreeMap<String, ByteSize>,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct Memory {
    pub total: ByteSize,
    pub free: ByteSize,
    pub platform_memory: PlatformMemory,
}

#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd"
))]
pub type PlatformSwap = PlatformMemory;

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformSwap {
    pub total: ByteSize,
    pub avail: ByteSize,
    pub used: ByteSize,
    pub pagesize: ByteSize,
    pub encrypted: bool,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct Swap {
    pub total: ByteSize,
    pub free: ByteSize,
    pub platform_swap: PlatformSwap,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct BatteryLife {
    pub remaining_capacity: f32,
    pub remaining_time: Duration,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct Filesystem {
    /// Used file nodes in filesystem
    pub files: usize,
    /// Total file nodes in filesystem
    pub files_total: usize,
    /// Free nodes available to non-superuser
    pub files_avail: usize,
    /// Free bytes in filesystem
    pub free: ByteSize,
    /// Free bytes available to non-superuser
    pub avail: ByteSize,
    /// Total bytes in filesystem
    pub total: ByteSize,
    /// Maximum filename length
    pub name_max: usize,
    pub fs_type: String,
    pub fs_mounted_from: String,
    pub fs_mounted_on: String,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct BlockDeviceStats {
    pub name: String,
    pub read_ios: usize,
    pub read_merges: usize,
    pub read_sectors: usize,
    pub read_ticks: usize,
    pub write_ios: usize,
    pub write_merges: usize,
    pub write_sectors: usize,
    pub write_ticks: usize,
    pub in_flight: usize,
    pub io_ticks: usize,
    pub time_in_queue: usize,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone, PartialEq)]
pub enum IpAddr {
    Empty,
    Unsupported,
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct NetworkAddrs {
    pub addr: IpAddr,
    pub netmask: IpAddr,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
    pub addrs: Vec<NetworkAddrs>,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub rx_bytes: ByteSize,
    pub tx_bytes: ByteSize,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct SocketStats {
    pub tcp_sockets_in_use: usize,
    pub tcp_sockets_orphaned: usize,
    pub udp_sockets_in_use: usize,
    pub tcp6_sockets_in_use: usize,
    pub udp6_sockets_in_use: usize,
}
