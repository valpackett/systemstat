use super::common::*;
use super::unix;
use data::*;
use libc::{c_int, c_void, size_t, statfs, sysctl, sysctlnametomib, timeval};
use std::process::Command;

use nom::not_line_ending;
use std::{ffi, io, mem, path, ptr, slice, str};
pub struct PlatformImpl;

macro_rules! sysctl_mib {
    ($len:expr, $name:expr) => {{
        let mut mib: [c_int; $len] = [0; $len];
        let mut sz: size_t = mib.len();
        let s = ffi::CString::new($name).unwrap();
        unsafe { sysctlnametomib(s.as_ptr(), &mut mib[0], &mut sz) };
        mib
    }};
}

macro_rules! sysctl {
    ($mib:expr, $dataptr:expr, $size:expr, $shouldcheck:expr) => {{
        let mib = &$mib;
        let mut size = $size;
        if unsafe {
            sysctl(
                &mib[0] as *const _ as *mut _,
                mib.len() as u32,
                $dataptr as *mut _ as *mut c_void,
                &mut size,
                ptr::null_mut(),
                0,
            )
        } != 0
            && $shouldcheck
        {
            return Err(io::Error::new(io::ErrorKind::Other, "sysctl() failed"));
        }
        size
    }};
    ($mib:expr, $dataptr:expr, $size:expr) => {
        sysctl!($mib, $dataptr, $size, true)
    };
}

named!(
    usize_s<usize>,
    ws!(map_res!(
        map_res!(nom::digit, str::from_utf8),
        str::FromStr::from_str
    ))
);

named!(
    proc_meminfo_line<(String, ByteSize)>,
    complete!(do_parse!(
        key: flat_map!(take_until!(":"), parse_to!(String))
            >> tag!(":")
            >> value: usize_s
            >> ws!(tag!("."))
            >> ((key, ByteSize::b(value * 4096)))
    ))
);

/// Optionally parse a `/proc/meminfo` line`
named!(
    proc_meminfo_line_opt<Option<(String, ByteSize)>>,
    opt!(proc_meminfo_line)
);

/// Parse `/proc/meminfo` into a hashmap
named!(
    proc_meminfo<BTreeMap<String, ByteSize>>,
    fold_many0!(
        ws!(flat_map!(not_line_ending, proc_meminfo_line_opt)),
        BTreeMap::new(),
        |mut map: BTreeMap<String, ByteSize>, opt| {
            if let Some((key, val)) = opt {
                map.insert(key, val);
            }
            map
        }
    )
);

lazy_static! {
    static ref KERN_BOOTTIME: [c_int; 2] = sysctl_mib!(2, "kern.boottime");
}

/// Get memory statistics
fn memory_stats() -> io::Result<BTreeMap<String, ByteSize>> {
    let data = Command::new("vm_stat")
        .output()
        .expect("Failed to execute vm_stat");

    proc_meminfo(&data.stdout)
        .to_result()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
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
        memory_stats().map(|meminfo| {
            let total_memory = *meminfo.get("Pages free").unwrap_or(&ByteSize::b(0))
                + *meminfo.get("Pages active").unwrap_or(&ByteSize::b(0))
                + *meminfo.get("Pages inactive").unwrap_or(&ByteSize::b(0))
                + *meminfo.get("Pages speculative").unwrap_or(&ByteSize::b(0));

            let free_memory = *meminfo.get("Pages free").unwrap_or(&ByteSize::b(0));
            let active_memory = *meminfo.get("Pages active").unwrap_or(&ByteSize::b(0));
            let inactive_memory = *meminfo.get("Pages inactive").unwrap_or(&ByteSize::b(0));
            let wired_memory = *meminfo.get("Pages wired down").unwrap_or(&ByteSize::b(0));
            // OSX does not cache like a regular linux system, and I am unsure how to extract
            // the needed info from the `vm_stat` command as of yet
            let cache_used = ByteSize::b(0);
            Memory {
                total: total_memory,
                free: free_memory,
                platform_memory: PlatformMemory {
                    active: active_memory,
                    free: free_memory,
                    inactive: inactive_memory,
                    wired: wired_memory,
                    cache: cache_used,
                },
            }
        })
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        let mut data: timeval = unsafe { mem::zeroed() };
        sysctl!(KERN_BOOTTIME, &mut data, mem::size_of::<timeval>());
        Ok(DateTime::<Utc>::from_utc(
            NaiveDateTime::from_timestamp(data.tv_sec, data.tv_usec as u32),
            Utc,
        ))
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
            return Err(io::Error::new(io::ErrorKind::Other, "getmntinfo() failed"));
        }
        let mounts = unsafe { slice::from_raw_parts(mptr, len as usize) };
        Ok(mounts.iter().map(statfs_to_fs).collect::<Vec<_>>())
    }

    fn mount_at<P: AsRef<path::Path>>(&self, _: P) -> io::Result<Filesystem> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
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
        files: x.f_files as usize - x.f_ffree as usize,
        files_total: x.f_files as usize,
        files_avail: x.f_ffree as usize,
        free: ByteSize::b(x.f_bfree as usize * x.f_bsize as usize),
        avail: ByteSize::b(x.f_bavail as usize * x.f_bsize as usize),
        total: ByteSize::b(x.f_blocks as usize * x.f_bsize as usize),
        name_max: 256,
        fs_type: unsafe {
            ffi::CStr::from_ptr(&x.f_fstypename[0])
                .to_string_lossy()
                .into_owned()
        },
        fs_mounted_from: unsafe {
            ffi::CStr::from_ptr(&x.f_mntfromname[0])
                .to_string_lossy()
                .into_owned()
        },
        fs_mounted_on: unsafe {
            ffi::CStr::from_ptr(&x.f_mntonname[0])
                .to_string_lossy()
                .into_owned()
        },
    }
}

#[link(name = "c")]
extern "C" {
    #[link_name = "getmntinfo$INODE64"]
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
}
