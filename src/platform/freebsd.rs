// You are likely to be eaten by a grue.

use std::{io, path, ptr, mem, ffi, slice, time};
use std::os::unix::ffi::OsStrExt;
use libc::{c_void, c_int, size_t, sysctl, sysctlnametomib, timeval, statfs};
use data::*;
use super::common::*;
use super::unix;
use super::bsd;

pub struct PlatformImpl;

macro_rules! sysctl_mib {
    ($len:expr, $name:expr) => {
        {
            let mut mib: [c_int; $len] = [0; $len];
            let mut sz: size_t = mib.len();
            let s = ffi::CString::new($name).unwrap();
            unsafe { sysctlnametomib(s.as_ptr(), &mut mib[0], &mut sz) };
            mib
        }
    }
}

macro_rules! sysctl {
    ($mib:expr, $dataptr:expr, $size:expr, $shouldcheck:expr) => {
        {
            let mib = &$mib;
            let mut size = $size;
            if unsafe { sysctl(&mib[0], mib.len() as u32,
                               $dataptr as *mut _ as *mut c_void, &mut size, ptr::null(), 0) } != 0 && $shouldcheck {
                return Err(io::Error::new(io::ErrorKind::Other, "sysctl() failed"))
            }
            size
        }
    };
    ($mib:expr, $dataptr:expr, $size:expr) => {
        sysctl!($mib, $dataptr, $size, true)
    }
}

lazy_static! {
    static ref KERN_CP_TIMES: [c_int; 2] = sysctl_mib!(2, "kern.cp_times");
    static ref KERN_BOOTTIME: [c_int; 2] = sysctl_mib!(2, "kern.boottime");
    static ref V_ACTIVE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_active_count");
    static ref V_INACTIVE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_inactive_count");
    static ref V_WIRE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_wire_count");
    static ref V_CACHE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_cache_count");
    static ref V_FREE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_free_count");
    static ref ZFS_ARC_SIZE: [c_int; 5] = sysctl_mib!(5, "kstat.zfs.misc.arcstats.size");
    static ref BATTERY_LIFE: [c_int; 4] = sysctl_mib!(4, "hw.acpi.battery.life");
    static ref BATTERY_TIME: [c_int; 4] = sysctl_mib!(4, "hw.acpi.battery.time");
    static ref ACLINE: [c_int; 3] = sysctl_mib!(3, "hw.acpi.acline");
    static ref CPU0TEMP: [c_int; 4] = sysctl_mib!(4, "dev.cpu.0.temperature");

    static ref CP_TIMES_SIZE: usize = {
        let mut size: usize = 0;
        unsafe { sysctl(&KERN_CP_TIMES[0], KERN_CP_TIMES.len() as u32,
                        ptr::null_mut(), &mut size, ptr::null(), 0) };
        size
    };
}

/// An implementation of `Platform` for FreeBSD.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        let loads = measure_cpu()?;
        Ok(DelayedMeasurement::new(
                Box::new(move || Ok(loads.iter()
                               .zip(measure_cpu()?.iter())
                               .map(|(prev, now)| (*now - prev).to_cpuload())
                               .collect::<Vec<_>>()))))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        unix::load_average()
    }

    fn memory(&self) -> io::Result<Memory> {
        let mut active: usize = 0; sysctl!(V_ACTIVE_COUNT, &mut active, mem::size_of::<usize>());
        let mut inactive: usize = 0; sysctl!(V_INACTIVE_COUNT, &mut inactive, mem::size_of::<usize>());
        let mut wired: usize = 0; sysctl!(V_WIRE_COUNT, &mut wired, mem::size_of::<usize>());
        let mut cache: usize = 0; sysctl!(V_CACHE_COUNT, &mut cache, mem::size_of::<usize>(), false);
        let mut free: usize = 0; sysctl!(V_FREE_COUNT, &mut free, mem::size_of::<usize>());
        let arc = ByteSize::b(zfs_arc_size().unwrap_or(0));
        let pmem = PlatformMemory {
            active: ByteSize::kib((active as u64) << *bsd::PAGESHIFT),
            inactive: ByteSize::kib((inactive as u64) << *bsd::PAGESHIFT),
            wired: saturating_sub_bytes(ByteSize::kib((wired as u64) << *bsd::PAGESHIFT), arc),
            cache: ByteSize::kib((cache as u64) << *bsd::PAGESHIFT),
            zfs_arc: arc,
            free: ByteSize::kib((free as u64) << *bsd::PAGESHIFT),
        };
        Ok(Memory {
            total: pmem.active + pmem.inactive + pmem.wired + pmem.cache + arc + pmem.free,
            free: pmem.inactive + pmem.cache + arc + pmem.free,
            platform_memory: pmem,
        })
    }

    fn swap(&self) -> io::Result<Swap> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        let mut data: timeval = unsafe { mem::zeroed() };
        sysctl!(KERN_BOOTTIME, &mut data, mem::size_of::<timeval>());
        Ok(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(data.tv_sec.into(), data.tv_usec as u32), Utc))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        let mut life: usize = 0; sysctl!(BATTERY_LIFE, &mut life, mem::size_of::<usize>());
        let mut time: i32 = 0; sysctl!(BATTERY_TIME, &mut time, mem::size_of::<i32>());
        Ok(BatteryLife {
            remaining_capacity: life as f32 / 100.0,
            remaining_time: time::Duration::from_secs(if time < 0 { 0 } else { time as u64 }),
        })
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        let mut on: usize = 0; sysctl!(ACLINE, &mut on, mem::size_of::<usize>());
        Ok(on == 1)
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        let mut mptr: *mut statfs = ptr::null_mut();
        let len = unsafe { getmntinfo(&mut mptr, 1 as i32) };
        if len < 1 {
            return Err(io::Error::new(io::ErrorKind::Other, "getmntinfo() failed"))
        }
        let mounts = unsafe { slice::from_raw_parts(mptr, len as usize) };
        Ok(mounts.iter().map(|m| statfs_to_fs(&m)).collect::<Vec<_>>())
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        let path = ffi::CString::new(path.as_ref().as_os_str().as_bytes())?;
        let mut sfs: statfs = unsafe { mem::zeroed() };
        if unsafe { statfs(path.as_ptr() as *const _, &mut sfs) } != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "statfs() failed"));
        }
        Ok(statfs_to_fs(&sfs))
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, _interface: &str) -> io::Result<NetworkStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        let mut temp: i32 = 0; sysctl!(CPU0TEMP, &mut temp, mem::size_of::<i32>());
        // The sysctl interface supports more units, but both amdtemp and coretemp always
        // use IK (deciKelvin)
        Ok((temp as f32 - 2731.5) / 10.0)
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }
}


fn measure_cpu() -> io::Result<Vec<CpuTime>> {
    let cpus = *CP_TIMES_SIZE / mem::size_of::<bsd::sysctl_cpu>();
    let mut data: Vec<bsd::sysctl_cpu> = Vec::with_capacity(cpus);
    unsafe { data.set_len(cpus) };

    sysctl!(KERN_CP_TIMES, &mut data[0], *CP_TIMES_SIZE);
    Ok(data.into_iter().map(|cpu| cpu.into()).collect())
}

fn zfs_arc_size() -> io::Result<u64> {
    let mut zfs_arc: usize = 0;
    sysctl!(ZFS_ARC_SIZE, &mut zfs_arc, mem::size_of::<usize>());
    Ok(zfs_arc as u64)
}

fn statfs_to_fs(fs: &statfs) -> Filesystem {
    Filesystem {
        files: (fs.f_files as usize).saturating_sub(fs.f_ffree as usize),
        files_total: fs.f_files as usize,
        files_avail: fs.f_ffree as usize,
        free: ByteSize::b(fs.f_bfree * fs.f_bsize),
        avail: ByteSize::b(fs.f_bavail as u64 * fs.f_bsize),
        total: ByteSize::b(fs.f_blocks * fs.f_bsize),
        name_max: fs.f_namemax as usize,
        fs_type: unsafe { ffi::CStr::from_ptr(&fs.f_fstypename[0]).to_string_lossy().into_owned() },
        fs_mounted_from: unsafe { ffi::CStr::from_ptr(&fs.f_mntfromname[0]).to_string_lossy().into_owned() },
        fs_mounted_on: unsafe { ffi::CStr::from_ptr(&fs.f_mntonname[0]).to_string_lossy().into_owned() },
    }
}

#[link(name = "c")]
extern "C" {
    #[link_name = "getmntinfo@FBSD_1.0"]
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
}
