use winapi::ctypes::c_char;
use winapi::shared::minwindef::*;
use winapi::um::{sysinfoapi, winbase};

mod disk;
mod network_interfaces;
mod socket;

use super::common::*;
use data::*;

use std::ffi::CStr;
use std::slice::from_raw_parts;
use std::{io, mem, path};

fn u16_array_to_string(p: *const u16) -> String {
    use std::char::{decode_utf16, REPLACEMENT_CHARACTER};
    unsafe {
        if p.is_null() {
            return String::new();
        }
        let mut amt = 0usize;
        while !p.offset(amt as isize).is_null() && *p.offset(amt as isize) != 0u16 {
            amt += 1;
        }
        let u16s = from_raw_parts(p, amt);
        decode_utf16(u16s.iter().cloned())
            .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
            .collect::<String>()
    }
}

fn c_char_array_to_string(p: *const c_char) -> String {
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

fn last_os_error() -> io::Result<()> {
    Err(io::Error::last_os_error())
}

pub struct PlatformImpl;

/// An implementation of `Platform` for Windows.
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
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn memory(&self) -> io::Result<Memory> {
        let mut status = sysinfoapi::MEMORYSTATUSEX {
            dwLength: mem::size_of::<sysinfoapi::MEMORYSTATUSEX>() as DWORD,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };
        unsafe {
            sysinfoapi::GlobalMemoryStatusEx(&mut status);
        }
        let pm = PlatformMemory {
            load: status.dwMemoryLoad,
            total_phys: ByteSize::b(status.ullTotalPhys as usize),
            avail_phys: ByteSize::b(status.ullAvailPhys as usize),
            total_pagefile: ByteSize::b(status.ullTotalPageFile as usize),
            avail_pagefile: ByteSize::b(status.ullAvailPageFile as usize),
            total_virt: ByteSize::b(status.ullTotalVirtual as usize),
            avail_virt: ByteSize::b(status.ullAvailVirtual as usize),
            avail_ext: ByteSize::b(status.ullAvailExtendedVirtual as usize),
        };
        Ok(Memory {
            total: pm.total_phys,
            free: pm.avail_phys,
            platform_memory: pm,
        })
    }

    fn uptime(&self) -> io::Result<Duration> {
        let since_boot: u64 = unsafe { sysinfoapi::GetTickCount64() };
        Ok(Duration::from_millis(since_boot))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        let status = power_status();
        if status.BatteryFlag == 128 {
            return Err(io::Error::new(io::ErrorKind::Other, "Battery absent"));
        }
        if status.BatteryFlag == 255 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Battery status unknown",
            ));
        }
        Ok(BatteryLife {
            remaining_capacity: status.BatteryLifePercent as f32 / 100.0,
            remaining_time: Duration::from_secs(status.BatteryLifeTime as u64),
        })
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Ok(power_status().ACLineStatus == 1)
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        disk::drives()
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        network_interfaces::get()
    }

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        socket::get()
    }
}

fn power_status() -> winbase::SYSTEM_POWER_STATUS {
    let mut status = winbase::SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        Reserved1: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    unsafe {
        winbase::GetSystemPowerStatus(&mut status);
    }
    status
}
