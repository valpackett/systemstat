// DragonFly BSD backend for systemstat.
//
// Implemented natively (no FreeBSD code reuse, since DragonFly's vm.stats
// layout, swap accounting and struct statfs all diverge):
//   * memory()        -- vm.stats.vm.v_*_count * pagesize
//   * swap()          -- vm.swap_size / vm.swap_{anon,cache}_use (bug #1805)
//   * load_average()  -- getloadavg(3)
//   * boot_time()     -- kern.boottime (uptime() falls out of the default impl)
//   * mounts()        -- getmntinfo(3)
//
// Left as ErrorKind::Unsupported for now. These need hardware/driver-specific
// sysctls or route/devstat walking, and starship does not consume them:
//   cpu_temp, battery_life,
//   block_device_statistics, network_stats, socket_stats

use std::collections::BTreeMap;
use std::{ffi, io, mem, ptr, slice};

use bytesize::ByteSize;
use libc::{c_char, c_int, c_long, c_void, statfs, timeval};

use super::common::*;
use super::unix;
use crate::data::*;

#[cfg(feature = "serde")]
use the_serde::{Deserialize, Serialize};

pub struct PlatformImpl;

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
    pub free: ByteSize,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "the_serde")
)]
#[derive(Debug, Clone)]
pub struct PlatformSwap {
    pub anon_use: ByteSize,
    pub cache_use: ByteSize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct DragonFlyCpuTime {
    user: c_long,
    nice: c_long,
    system: c_long,
    interrupt: c_long,
    idle: c_long,
}

impl From<DragonFlyCpuTime> for CpuTime {
    fn from(cpu: DragonFlyCpuTime) -> CpuTime {
        CpuTime {
            user: cpu.user as usize,
            nice: cpu.nice as usize,
            system: cpu.system as usize,
            interrupt: cpu.interrupt as usize,
            idle: cpu.idle as usize,
            other: 0,
        }
    }
}

const MNT_WAIT: c_int = 1;

/// Read a single fixed-size sysctl value by name.
///
/// `T` must be a plain-old-data type whose all-zero bit pattern is valid
/// (integers, `timeval`, ...). Reading a 4-byte kernel field into a zeroed
/// 8-byte `c_long` is safe on little-endian amd64: sysctl writes only the
/// kernel's actual length and the high bytes stay zero. DragonFly is amd64
/// only, so this holds.
unsafe fn sysctl_scalar<T: Copy>(name: &str) -> io::Result<T> {
    let cname = ffi::CString::new(name).unwrap();
    let mut value: T = mem::zeroed();
    let mut len: usize = mem::size_of::<T>();
    let rc = libc::sysctlbyname(
        cname.as_ptr(),
        &mut value as *mut T as *mut c_void,
        &mut len,
        ptr::null_mut(),
        0,
    );
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value)
}

fn sysctl_buffer_len(name: &str) -> io::Result<usize> {
    let cname = ffi::CString::new(name).unwrap();
    let mut len: usize = 0;
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            ptr::null_mut(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(len)
}

fn measure_cpu() -> io::Result<Vec<CpuTime>> {
    let len = sysctl_buffer_len("kern.cp_times")?;
    let cpu_size = mem::size_of::<DragonFlyCpuTime>();
    if len == 0 || len % cpu_size != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kern.cp_times returned an invalid size",
        ));
    }

    let cpus = len / cpu_size;
    let mut data = vec![
        DragonFlyCpuTime {
            user: 0,
            nice: 0,
            system: 0,
            interrupt: 0,
            idle: 0,
        };
        cpus
    ];
    unsafe {
        let cname = ffi::CString::new("kern.cp_times").unwrap();
        let mut actual_len = len;
        let rc = libc::sysctlbyname(
            cname.as_ptr(),
            data.as_mut_ptr() as *mut c_void,
            &mut actual_len,
            ptr::null_mut(),
            0,
        );
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        if actual_len != len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "kern.cp_times changed size while reading",
            ));
        }
    }

    Ok(data.into_iter().map(CpuTime::from).collect())
}

#[inline]
fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "systemstat: not yet implemented on DragonFly BSD",
    ))
}

#[inline]
fn cstr(buf: &[c_char]) -> String {
    unsafe {
        ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

impl Platform for PlatformImpl {
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        let loads = measure_cpu()?;
        Ok(DelayedMeasurement::new(Box::new(move || {
            Ok(loads
                .iter()
                .zip(measure_cpu()?.iter())
                .map(|(prev, now)| (*now - prev).to_cpuload())
                .collect::<Vec<_>>())
        })))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        let mut loads = [0f64; 3];
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        if n != 3 {
            return Err(io::Error::last_os_error());
        }
        Ok(LoadAverage {
            one: loads[0] as f32,
            five: loads[1] as f32,
            fifteen: loads[2] as f32,
        })
    }

    fn memory(&self) -> io::Result<Memory> {
        let ps = unsafe { getpagesize() } as u64;
        let pages = |name: &str| -> io::Result<u64> {
            Ok(unsafe { sysctl_scalar::<c_long>(name)? } as u64)
        };

        let active = pages("vm.stats.vm.v_active_count")?;
        let inactive = pages("vm.stats.vm.v_inactive_count")?;
        let wired = pages("vm.stats.vm.v_wire_count")?;
        let cache = pages("vm.stats.vm.v_cache_count")?;
        let free = pages("vm.stats.vm.v_free_count")?;
        let total = pages("vm.stats.vm.v_page_count")?;

        let pmem = PlatformMemory {
            active: ByteSize::b(active * ps),
            inactive: ByteSize::b(inactive * ps),
            wired: ByteSize::b(wired * ps),
            cache: ByteSize::b(cache * ps),
            free: ByteSize::b(free * ps),
        };

        Ok(Memory {
            total: ByteSize::b(total * ps),
            // "Available" memory: reclaimable pages. Mirrors the BSD convention
            // of inactive + cache + free.
            free: ByteSize::b((inactive + cache + free) * ps),
            platform_memory: pmem,
        })
    }

    fn swap(&self) -> io::Result<Swap> {
        // Per DragonFly bug #1805 (Dillon): free = swap_size - anon_use - cache_use.
        // All three are page counts. Usage sysctls may be absent on a system with
        // no swap configured, so degrade those to 0 rather than failing outright.
        let ps = unsafe { getpagesize() } as u64;
        let total = unsafe { sysctl_scalar::<c_long>("vm.swap_size")? } as u64;
        let anon = unsafe { sysctl_scalar::<c_long>("vm.swap_anon_use") }
            .map(|v| v as u64)
            .unwrap_or(0);
        let cache = unsafe { sysctl_scalar::<c_long>("vm.swap_cache_use") }
            .map(|v| v as u64)
            .unwrap_or(0);
        let used = anon + cache;

        Ok(Swap {
            total: ByteSize::b(total * ps),
            free: ByteSize::b(total.saturating_sub(used) * ps),
            platform_swap: PlatformSwap {
                anon_use: ByteSize::b(anon * ps),
                cache_use: ByteSize::b(cache * ps),
            },
        })
    }

    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        let bt: timeval = unsafe { sysctl_scalar("kern.boottime")? };
        OffsetDateTime::from_unix_timestamp(bt.tv_sec as i64)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        unsupported()
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        match unsafe { sysctl_scalar::<c_long>("hw.acpi.acline") } {
            Ok(1) => Ok(true),
            Ok(0) => Ok(false),
            Ok(_) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(err) => Err(err),
        }
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        let mut buf: *mut statfs = ptr::null_mut();
        let count = unsafe { getmntinfo(&mut buf, MNT_WAIT) };
        if count < 1 {
            return Err(io::Error::last_os_error());
        }
        let entries = unsafe { slice::from_raw_parts(buf, count as usize) };

        Ok(entries
            .iter()
            .map(|m| {
                let bsize = m.f_bsize as u64;
                let bfree = (m.f_bfree.max(0)) as u64;
                let bavail = (m.f_bavail.max(0)) as u64;
                let blocks = m.f_blocks as u64;
                let files_total = m.f_files as u64;
                let files_free = m.f_ffree.max(0) as u64;
                Filesystem {
                    files: files_total.saturating_sub(files_free) as usize,
                    files_total: files_total as usize,
                    files_avail: files_free as usize,
                    free: ByteSize::b(bfree * bsize),
                    avail: ByteSize::b(bavail * bsize),
                    total: ByteSize::b(blocks * bsize),
                    // DragonFly's struct statfs has no f_namemax; NAME_MAX is 255.
                    name_max: 255,
                    fs_type: cstr(&m.f_fstypename),
                    fs_mounted_from: cstr(&m.f_mntfromname),
                    fs_mounted_on: cstr(&m.f_mntonname),
                }
            })
            .collect())
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        unsupported()
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, _interface: &str) -> io::Result<NetworkStats> {
        unsupported()
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        unsupported()
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        unsupported()
    }
}

#[link(name = "c")]
extern "C" {
    fn getpagesize() -> c_int;
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
}
