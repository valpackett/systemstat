use std::{io, path, ptr, mem, ffi, slice, time, fs};
use std::path::PathBuf;
use std::io::Read;
use std::ops::Sub;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::collections::BTreeMap;
use std::time::Duration;
use libc::{c_void, c_int, c_ulong, c_ushort, c_uint, c_long, c_schar, c_uchar, size_t, uid_t};
use data::*;
use super::common::*;

// utility functions:
fn value_from_file(path: String) -> i32 {
    let mut val = String::new();
    fs::File::open(path).unwrap().read_to_string(&mut val);
    val.trim_right_matches("\n").parse().unwrap()
}

fn capacity(charge_full: i32, charge_now: i32) -> f32 {
    charge_now as f32 / charge_full as f32
}

fn time(charge_full: i32, charge_now: i32, current_now: i32) -> Duration {
    Duration::from_secs((charge_full - charge_now).abs() as u64 * 3600u64 / current_now as u64)
}

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
            return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"));
        }
        Ok(LoadAverage {
            one: loads[0] as f32,
            five: loads[1] as f32,
            fifteen: loads[2] as f32,
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
        let dir = "/sys/class/power_supply";
        let entries = fs::read_dir(&dir).unwrap();
        let mut full = 0;
        let mut now = 0;
        let mut current = 0;
        for e in entries {
            let p = e.unwrap().path();
            let s = p.to_str().unwrap();
            let f = p.file_name().unwrap().to_str().unwrap();
            if f.len() > 3 {
                if f.split_at(3).0 == "BAT" {
                    full += value_from_file(s.to_string() + "/charge_full");
                    now += value_from_file(s.to_string() + "/charge_now");
                    current += value_from_file(s.to_string() + "/current_now");
                }
            }
        }
        if full != 0 {
            Ok(BatteryLife {
                remaining_capacity: capacity(full, now),
                remaining_time: time(full, now, current),
            })
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
        }
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        let mut s = String::new();
        match fs::File::open("/sys/class/power_supply/AC/online").unwrap().read_to_string(&mut s)/*.expect("Failed to read file")*/ {
		    Ok(_) => { Ok(s.split_at(1).0 != "0".to_string()) }
		    Err(e) => { Err(io::Error::new(io::ErrorKind::Other, format!("Error: {} in function: on_ac_power()", e))) }
		}
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
extern "C" {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
    fn sysinfo(info: *mut sysinfo);
}
