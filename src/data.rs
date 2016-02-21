use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone)]
pub struct CPULoad {
    pub user_percent: f32,
    pub nice_percent: f32,
    pub system_percent: f32,
    pub interrupt_percent: f32,
    pub idle_percent: f32,
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
