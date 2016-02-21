//! This module provides the data structures that represent system information.
//!
//! They're always the same across all platforms.

use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};

/// A wrapper for a measurement that takes time.
///
/// Time should pass between getting the object and calling .done() on it.
pub struct DelayedMeasurement<T> {
    res: Box<Fn() -> io::Result<T>>
}

impl<T> DelayedMeasurement<T> {
    pub fn new(f: Box<Fn() -> io::Result<T>>) -> DelayedMeasurement<T> {
        DelayedMeasurement { res: f }
    }

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

#[derive(Debug, Clone)]
pub struct Filesystem {
    pub files: u64,
    pub free_bytes: u64,
    pub avail_bytes: u64,
    pub total_bytes: u64,
    pub name_max: u64,
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
