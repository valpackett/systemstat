use std::{io, path, ptr, mem, ffi, slice, time};
use std::ops::Sub;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::collections::BTreeMap;
use libc::{c_void, c_int, c_ulong, c_ushort, c_uint, c_long, c_schar, c_uchar, size_t, uid_t};
use data::*;
use super::common::*;

pub struct PlatformImpl;

/// An implementation of `Platform` for Linux.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        let mut loads: [f64; 3] = [0.0, 0.0, 0.0];
        if unsafe { getloadavg(&mut loads[0], 3) } != 3 {
            return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"))
        }
        Ok(LoadAverage {
            one: loads[0] as f32, five: loads[1] as f32, fifteen: loads[2] as f32
        })
    }

    fn memory(&self) -> io::Result<Memory> {
        let mut info: sysinfo = unsafe { mem::zeroed() };
        unsafe { sysinfo(&mut info) };
        let unit = info.mem_unit as usize;
        let pmem = PlatformMemory {
            total: ByteSize::b(info.totalram as usize * unit),
            free: ByteSize::b(info.freeram as usize * unit),
            shared: ByteSize::b(info.sharedram as usize * unit),
            buffer: ByteSize::b(info.bufferram as usize * unit),
        };
        Ok(Memory {
            total: pmem.total,
            free: pmem.free,
            platform_memory: pmem,
        })
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }
}

#[repr(C)]
#[derive(Debug)]
struct sysinfo {
    uptime: c_long,
    loads: [c_ulong; 3],
    totalram: c_ulong,
    freeram: c_ulong,
    sharedram: c_ulong,
    bufferram: c_ulong,
    totalswap: c_ulong,
    freeswap: c_ulong,
    procs: c_ushort,
    totalhigh: c_ulong,
    freehigh: c_ulong,
    mem_unit: c_uint,
    padding: [c_schar; 8],
}

#[link(name = "c")]
extern {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
    fn sysinfo(info: *mut sysinfo);
}
