use winapi::ctypes::c_char;
use winapi::shared::minwindef::*;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::{sysinfoapi, winbase};
use winapi::um::pdh::{
    PDH_FMT_COUNTERVALUE_ITEM_A,
    PDH_FMT_DOUBLE,
    PDH_HCOUNTER,
    PDH_HQUERY,
    PDH_FMT_NOCAP100,
    PdhAddEnglishCounterA,
    PdhCloseQuery,
    PdhCollectQueryData,
    PdhGetFormattedCounterArrayA,
    PdhOpenQueryA,
};

mod disk;
mod network_interfaces;
mod socket;

use super::common::*;
use crate::data::*;

use std::ffi::CStr;
use std::slice::from_raw_parts;
use std::cmp;
use std::{io, mem, path};

fn u16_array_to_string(p: *const u16) -> String {
    use std::char::{decode_utf16, REPLACEMENT_CHARACTER};
    unsafe {
        if p.is_null() {
            return String::new();
        }
        let mut amt = 0usize;
        while !p.add(amt).is_null() && *p.add(amt) != 0u16 {
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
        const PDH_MORE_DATA: u32 = 0x8000_07D2;

        struct QueryHandle(PDH_HQUERY);

        // Pdh is supposedly synchronized internally with a mutex
        unsafe impl Send for QueryHandle {}
        unsafe impl Sync for QueryHandle {}

        impl Drop for QueryHandle {
            fn drop(&mut self){
                unsafe {
                    PdhCloseQuery(self.0);
                }
            }
        }

        struct CounterHandle(PDH_HCOUNTER);

        unsafe impl Send for CounterHandle {}
        unsafe impl Sync for CounterHandle {}

        struct PerformanceCounter {
            query: QueryHandle,
            counter: CounterHandle,
        }

        impl PerformanceCounter {
            pub fn new(key: &CStr) -> io::Result<Self> {
                let mut query = std::ptr::null_mut();
                let status = unsafe {
                    PdhOpenQueryA(std::ptr::null(), 0, &mut query)
                };

                if status as u32 != ERROR_SUCCESS {
                    return Err(io::Error::from_raw_os_error(status));
                }

                let query = QueryHandle(query);

                let mut counter = std::ptr::null_mut();
                let status = unsafe {
                    PdhAddEnglishCounterA(query.0, key.as_ptr(), 0, &mut counter)
                };

                if status as u32 != ERROR_SUCCESS {
                    return Err(io::Error::from_raw_os_error(status));
                }

                let counter = CounterHandle(counter);

                let status = unsafe {
                    PdhCollectQueryData(query.0)
                };

                if status as u32 != ERROR_SUCCESS {
                    return Err(io::Error::from_raw_os_error(status));
                }

                Ok(Self {
                    query,
                    counter,
                })
            }

            fn next_value(&self) -> io::Result<Vec<PDH_FMT_COUNTERVALUE_ITEM_A>> {
                let status = unsafe {
                    PdhCollectQueryData(self.query.0)
                };

                if status as u32 != ERROR_SUCCESS {
                    return Err(io::Error::from_raw_os_error(status));
                }

                let mut buffer_size = 0;
                let mut item_count = 0;
                let status = unsafe {
                    PdhGetFormattedCounterArrayA(self.counter.0, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, &mut buffer_size, &mut item_count, std::ptr::null_mut())
                };

                match status as u32 {
                    PDH_MORE_DATA => {},
                    ERROR_SUCCESS => {
                        return Ok(Vec::new());
                    }
                    _ => {
                        return Err(io::Error::from_raw_os_error(status));
                    }
                }

                let mut items = Vec::new();
                items.reserve(item_count as usize * std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_A>());
                let status = unsafe {
                    PdhGetFormattedCounterArrayA(self.counter.0, PDH_FMT_DOUBLE, &mut buffer_size, &mut item_count, items.as_mut_ptr())
                };

                if status as u32 != ERROR_SUCCESS {
                    return Err(io::Error::from_raw_os_error(status));
                }

                unsafe {
                    items.set_len(item_count as usize);
                }

                Ok(items)
            }
        }

        let user_counter = PerformanceCounter::new(CStr::from_bytes_with_nul(b"\\Processor(*)\\% User Time\0").unwrap())?;
        let idle_counter = PerformanceCounter::new(CStr::from_bytes_with_nul(b"\\Processor(*)\\% Idle Time\0").unwrap())?;
        let system_counter = PerformanceCounter::new(CStr::from_bytes_with_nul(b"\\Processor(*)\\% Privileged Time\0").unwrap())?;
        let interrupt_counter = PerformanceCounter::new(CStr::from_bytes_with_nul(b"\\Processor(*)\\% Interrupt Time\0").unwrap())?;

        Ok(DelayedMeasurement::new(Box::new(move || {
            let user = user_counter.next_value()?;
            let idle = idle_counter.next_value()?;
            let system = system_counter.next_value()?;
            let interrupt = interrupt_counter.next_value()?;

            let count = user.iter().filter(|item| unsafe { CStr::from_ptr(item.szName).to_string_lossy() } != "_Total").count();

            let mut ret = vec![
                CPULoad {
                    user: 0.0,
                    nice: 0.0,
                    system: 0.0,
                    interrupt: 0.0,
                    idle: 0.0,
                    platform: PlatformCpuLoad {},
                };
                count
            ];

            for item in user {
                let name = unsafe { CStr::from_ptr(item.szName).to_string_lossy() };
                if let Ok(n) = name.parse::<usize>(){
                    ret[n].user = unsafe { (*item.FmtValue.u.doubleValue() / 100.0) as f32 };
                }
            }

            for item in idle {
                let name = unsafe { CStr::from_ptr(item.szName).to_string_lossy() };
                if let Ok(n) = name.parse::<usize>(){
                    ret[n].idle = unsafe { (*item.FmtValue.u.doubleValue() / 100.0) as f32 };
                }
            }

            for item in system {
                let name = unsafe { CStr::from_ptr(item.szName).to_string_lossy() };
                if let Ok(n) = name.parse::<usize>(){
                    ret[n].system = unsafe { (*item.FmtValue.u.doubleValue() / 100.0) as f32 };
                }
            }

            for item in interrupt {
                let name = unsafe { CStr::from_ptr(item.szName).to_string_lossy() };
                if let Ok(n) = name.parse::<usize>(){
                    ret[n].interrupt = unsafe { (*item.FmtValue.u.doubleValue() / 100.0) as f32 };
                }
            }

            Ok(ret)
        })))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn memory(&self) -> io::Result<Memory> {
        PlatformMemory::new().map(|pm| pm.to_memory())
    }

    fn swap(&self) -> io::Result<Swap> {
        PlatformMemory::new().map(|pm| pm.to_swap())
    }

    fn memory_and_swap(&self) -> io::Result<(Memory, Swap)> {
        let pm = PlatformMemory::new()?;
        Ok((pm.clone().to_memory(), pm.to_swap()))
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

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        network_interfaces::get()
    }

    fn network_stats(&self, _interface: &str) -> io::Result<NetworkStats> {
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

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
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
        let ret = unsafe {
            sysinfoapi::GlobalMemoryStatusEx(&mut status)
        };
        if ret == 0 {
            return Err(io::Error::last_os_error())
        }

        Ok(Self {
            load: status.dwMemoryLoad,
            total_phys: ByteSize::b(status.ullTotalPhys),
            avail_phys: ByteSize::b(status.ullAvailPhys),
            total_pagefile: ByteSize::b(status.ullTotalPageFile),
            avail_pagefile: ByteSize::b(status.ullAvailPageFile),
            total_virt: ByteSize::b(status.ullTotalVirtual),
            avail_virt: ByteSize::b(status.ullAvailVirtual),
            avail_ext: ByteSize::b(status.ullAvailExtendedVirtual),
        })
    }

    // Convert the platform memory information to Memory
    fn to_memory(self) -> Memory {
        Memory {
            total: self.total_phys,
            free: self.avail_phys,
            platform_memory: self,
        }
    }

    // Convert the platform memory information to Swap
    fn to_swap(self) -> Swap {
        // Be catious because pagefile and phys don't always sync up
        // Despite the name, pagefile includes both physical and swap memory
        let total = saturating_sub_bytes(self.total_pagefile, self.total_phys);
        let free = saturating_sub_bytes(self.avail_pagefile, self.avail_phys);
        Swap {
            total,
            // Sometimes, especially when swap total is 0, free can exceed total
            free: cmp::min(total, free),
            platform_swap: self,
        }
    }
}
