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
// Left as ErrorKind::Unsupported for now. These need route/devstat walking or
// socket-table aggregation, and starship does not consume them:
//   block_device_statistics, socket_stats

use std::collections::BTreeMap;
use std::{ffi, io, mem, ptr, slice};

use bytesize::ByteSize;
use libc::{c_char, c_int, c_long, c_ulong, c_void, statfs, timeval};

use super::common::*;
use super::unix;
use crate::data::*;

#[cfg(feature = "serde")]
use the_serde::{Deserialize, Serialize};

pub struct PlatformImpl;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IfData {
    ifi_type: u8,
    ifi_physical: u8,
    ifi_addrlen: u8,
    ifi_hdrlen: u8,
    ifi_recvquota: u8,
    ifi_xmitquota: u8,
    ifi_mtu: c_ulong,
    ifi_metric: c_ulong,
    ifi_link_state: c_ulong,
    ifi_baudrate: u64,
    ifi_ipackets: c_ulong,
    ifi_ierrors: c_ulong,
    ifi_opackets: c_ulong,
    ifi_oerrors: c_ulong,
    ifi_collisions: c_ulong,
    ifi_ibytes: c_ulong,
    ifi_obytes: c_ulong,
    ifi_imcasts: c_ulong,
    ifi_omcasts: c_ulong,
    ifi_iqdrops: c_ulong,
    ifi_noproto: c_ulong,
    ifi_hwassist: c_ulong,
    ifi_oqdrops: c_ulong,
    ifi_lastchange: timeval,
}

#[repr(C)]
struct IfReqData {
    ifr_name: [c_char; IFNAMSIZ],
    ifr_data: *mut IfData,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Sensor {
    desc: [c_char; 32],
    tv: timeval,
    value: i64,
    sensor_type: c_int,
    status: c_int,
    numt: c_int,
    flags: c_int,
}

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

const IFNAMSIZ: usize = 16;
const MNT_WAIT: c_int = 1;
const SENSOR_TEMP: c_int = 0;
const SENSOR_FINVALID: c_int = 0x0001;
const SENSOR_FUNKNOWN: c_int = 0x0002;
const SIOCGIFDATA: c_ulong = 0xc0206926;

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

fn read_sensor(name: &str) -> io::Result<Sensor> {
    unsafe { sysctl_scalar::<Sensor>(name) }
}

fn sensor_temp_celsius(sensor: Sensor) -> Option<f32> {
    if sensor.sensor_type != SENSOR_TEMP || sensor.flags & (SENSOR_FINVALID | SENSOR_FUNKNOWN) != 0
    {
        return None;
    }
    Some((sensor.value as f64 / 1_000_000.0 - 273.15) as f32)
}

fn first_sensor_temp(prefix: &str) -> Option<f32> {
    for dev in 0..16 {
        for index in 0..16 {
            let name = format!("hw.sensors.{}{}.temp{}", prefix, dev, index);
            if let Ok(sensor) = read_sensor(&name) {
                if let Some(temp) = sensor_temp_celsius(sensor) {
                    return Some(temp);
                }
            }
        }
    }
    None
}

fn copy_interface_name(dst: &mut [c_char; IFNAMSIZ], interface: &str) -> io::Result<()> {
    let bytes = interface.as_bytes();
    if bytes.len() >= IFNAMSIZ {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "interface name is too long",
        ));
    }
    for (idx, byte) in bytes.iter().enumerate() {
        dst[idx] = *byte as c_char;
    }
    Ok(())
}

fn if_data(interface: &str) -> io::Result<IfData> {
    let mut req = IfReqData {
        ifr_name: [0; IFNAMSIZ],
        ifr_data: ptr::null_mut(),
    };
    copy_interface_name(&mut req.ifr_name, interface)?;

    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    let mut data: IfData = unsafe { mem::zeroed() };
    req.ifr_data = &mut data;

    let rc = unsafe { libc::ioctl(fd, SIOCGIFDATA, &mut req) };
    let close_rc = unsafe { libc::close(fd) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    if close_rc != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(data)
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
        let life = match unsafe { sysctl_scalar::<c_long>("hw.acpi.battery.life") } {
            Ok(life) => life,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return unsupported(),
            Err(err) => return Err(err),
        };
        let time = match unsafe { sysctl_scalar::<c_long>("hw.acpi.battery.time") } {
            Ok(time) => time,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return unsupported(),
            Err(err) => return Err(err),
        };
        Ok(BatteryLife {
            remaining_capacity: life as f32 / 100.0,
            remaining_time: Duration::from_secs(time.max(0) as u64 * 60),
        })
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

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
        let data = if_data(interface)?;
        Ok(NetworkStats {
            rx_bytes: ByteSize::b(data.ifi_ibytes as u64),
            tx_bytes: ByteSize::b(data.ifi_obytes as u64),
            rx_packets: data.ifi_ipackets as u64,
            tx_packets: data.ifi_opackets as u64,
            rx_errors: data.ifi_ierrors as u64,
            tx_errors: data.ifi_oerrors as u64,
        })
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        for prefix in ["die", "coretemp", "amdtemp", "cpu", "acpitz", "lm"] {
            if let Some(temp) = first_sensor_temp(prefix) {
                return Ok(temp);
            }
        }
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
