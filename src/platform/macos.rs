use std::{io, ptr, mem::{self, MaybeUninit}, ffi, slice};
use libc::{
    c_int, c_void, host_statistics64, mach_host_self, size_t, statfs, sysconf, sysctl,
    sysctlnametomib, timeval, vm_statistics64, HOST_VM_INFO64, HOST_VM_INFO64_COUNT, KERN_SUCCESS,
    _SC_PHYS_PAGES,
};
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
            if unsafe { sysctl(&mib[0] as *const _ as *mut _, mib.len() as u32,
                               $dataptr as *mut _ as *mut c_void, &mut size, ptr::null_mut(), 0) } != 0 && $shouldcheck {
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
    static ref KERN_BOOTTIME: [c_int; 2] = sysctl_mib!(2, "kern.boottime");
}

/// An implementation of `Platform` for macOS.
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
        unix::load_average()
    }

    fn memory(&self) -> io::Result<Memory> {
        // Get Total Memory
        let total = match unsafe { sysconf(_SC_PHYS_PAGES) } {
            -1 => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "sysconf(_SC_PHYS_PAGES) failed",
                ))
            }
            n => n as u64,
        };

        // Get Usage Info
        let host_port = unsafe { mach_host_self() };
        let mut stat = MaybeUninit::<vm_statistics64>::zeroed();
        let mut stat_count = HOST_VM_INFO64_COUNT;

        let ret = unsafe {
            host_statistics64(
                host_port,
                HOST_VM_INFO64,
                stat.as_mut_ptr() as *mut i32,
                &mut stat_count,
            )
        };

        if ret != KERN_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "host_statistics64() failed",
            ));
        }
        let stat = unsafe { stat.assume_init() };

        let pmem = PlatformMemory {
            total: ByteSize::kib(total << *bsd::PAGESHIFT),
            active: ByteSize::kib((stat.active_count as u64) << *bsd::PAGESHIFT),
            inactive: ByteSize::kib((stat.inactive_count as u64) << *bsd::PAGESHIFT),
            wired: ByteSize::kib((stat.wire_count as u64) << *bsd::PAGESHIFT),
            free: ByteSize::kib((stat.free_count as u64) << *bsd::PAGESHIFT),
            purgeable: ByteSize::kib((stat.purgeable_count as u64) << *bsd::PAGESHIFT),
            speculative: ByteSize::kib((stat.speculative_count as u64) << *bsd::PAGESHIFT),
            compressor: ByteSize::kib((stat.compressor_page_count as u64) << *bsd::PAGESHIFT),
            throttled: ByteSize::kib((stat.throttled_count as u64) << *bsd::PAGESHIFT),
            external: ByteSize::kib((stat.external_page_count as u64) << *bsd::PAGESHIFT),
            internal: ByteSize::kib((stat.internal_page_count as u64) << *bsd::PAGESHIFT),
            uncompressed_in_compressor: ByteSize::kib(
                (stat.total_uncompressed_pages_in_compressor as u64) << *bsd::PAGESHIFT,
            ),
        };

        Ok(Memory {
            total: pmem.total,
            // This is the available memory, but free is more akin to:
            // pmem.free - pmem.speculative
            free: pmem.free + pmem.inactive,
            platform_memory: pmem,
        })
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        let mut data: timeval = unsafe { mem::zeroed() };
        sysctl!(KERN_BOOTTIME, &mut data, mem::size_of::<timeval>());
        Ok(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(data.tv_sec.into(), data.tv_usec as u32), Utc))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        let mut mptr: *mut statfs = ptr::null_mut();
        let len = unsafe { getmntinfo(&mut mptr, 2 as i32) };
        if len < 1 {
            return Err(io::Error::new(io::ErrorKind::Other, "getmntinfo() failed"))
        }
        let mounts = unsafe { slice::from_raw_parts(mptr, len as usize) };
        Ok(mounts.iter().map(statfs_to_fs).collect::<Vec<_>>())
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
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }
}

fn statfs_to_fs(x: &statfs) -> Filesystem {
    Filesystem {
        files: (x.f_files as usize).saturating_sub(x.f_ffree as usize),
        files_total: x.f_files as usize,
        files_avail: x.f_ffree as usize,
        free: ByteSize::b(x.f_bfree * x.f_bsize as u64),
        avail: ByteSize::b(x.f_bavail * x.f_bsize as u64),
        total: ByteSize::b(x.f_blocks * x.f_bsize as u64),
        name_max: 256,
        fs_type: unsafe { ffi::CStr::from_ptr(&x.f_fstypename[0]).to_string_lossy().into_owned() },
        fs_mounted_from: unsafe { ffi::CStr::from_ptr(&x.f_mntfromname[0]).to_string_lossy().into_owned() },
        fs_mounted_on: unsafe { ffi::CStr::from_ptr(&x.f_mntonname[0]).to_string_lossy().into_owned() },
    }
}

#[link(name = "c")]
extern "C" {
    #[cfg_attr(not(target_arch = "aarch64"), link_name = "getmntinfo$INODE64")]
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
}
