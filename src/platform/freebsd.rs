// You are likely to be eaten by a grue.

use std::{io, path, ptr, mem, ffi, slice, time};
use std::ops::Sub;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::collections::BTreeMap;
use libc::{c_void, c_int, c_schar, c_uchar, size_t, uid_t, sysctl, sysctlnametomib,
           getifaddrs, freeifaddrs, ifaddrs, sockaddr, sockaddr_in6, AF_INET, AF_INET6};
use data::*;
use super::common::*;

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
    ($mib:expr, $dataptr:expr, $size:expr) => {
        {
            let mib = &$mib;
            let mut size = $size;
            if unsafe { sysctl(&mib[0], mib.len() as u32,
                               $dataptr as *mut _ as *mut c_void, &mut size, ptr::null(), 0) } != 0 {
                return Err(io::Error::new(io::ErrorKind::Other, "sysctl() failed"))
            }
            size
        }
    }
}

lazy_static! {
    static ref PAGESHIFT: c_int = {
        let mut pagesize = unsafe { getpagesize() };
        let mut pageshift = 0;
        while pagesize > 1 {
            pageshift += 1;
            pagesize >>= 1;
        }
        pageshift - 10 // LOG1024
    };

    static ref KERN_CP_TIMES: [c_int; 2] = sysctl_mib!(2, "kern.cp_times");
    static ref V_ACTIVE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_active_count");
    static ref V_INACTIVE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_inactive_count");
    static ref V_WIRE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_wire_count");
    static ref V_CACHE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_cache_count");
    static ref V_FREE_COUNT: [c_int; 4] = sysctl_mib!(4, "vm.stats.vm.v_free_count");
    static ref BATTERY_LIFE: [c_int; 4] = sysctl_mib!(4, "hw.acpi.battery.life");
    static ref BATTERY_TIME: [c_int; 4] = sysctl_mib!(4, "hw.acpi.battery.time");
    static ref ACLINE: [c_int; 3] = sysctl_mib!(3, "hw.acpi.acline");

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
        let loads = try!(sysctl_cpu::measure());
        Ok(DelayedMeasurement::new(
                Box::new(move || Ok(loads.iter()
                               .zip(try!(sysctl_cpu::measure()).iter())
                               .map(|(prev, now)| (*now - prev).to_cpuload())
                               .collect::<Vec<_>>()))))
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
        let mut active: usize = 0; sysctl!(V_ACTIVE_COUNT, &mut active, mem::size_of::<usize>());
        let mut inactive: usize = 0; sysctl!(V_INACTIVE_COUNT, &mut inactive, mem::size_of::<usize>());
        let mut wired: usize = 0; sysctl!(V_WIRE_COUNT, &mut wired, mem::size_of::<usize>());
        let mut cache: usize = 0; sysctl!(V_CACHE_COUNT, &mut cache, mem::size_of::<usize>());
        let mut free: usize = 0; sysctl!(V_FREE_COUNT, &mut free, mem::size_of::<usize>());
        let pmem = PlatformMemory {
            active: ByteSize::kib(active << *PAGESHIFT),
            inactive: ByteSize::kib(inactive << *PAGESHIFT),
            wired: ByteSize::kib(wired << *PAGESHIFT),
            cache: ByteSize::kib(cache << *PAGESHIFT),
            free: ByteSize::kib(free << *PAGESHIFT),
        };
        Ok(Memory {
            total: pmem.active + pmem.inactive + pmem.wired + pmem.cache + pmem.free,
            free: pmem.inactive + pmem.cache + pmem.free,
            platform_memory: pmem,
        })
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
        Ok(mounts.iter().map(|m| m.to_fs()).collect::<Vec<_>>())
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        let mut sfs: statfs = unsafe { mem::zeroed() };
        if unsafe { statfs(path.as_ref().to_string_lossy().as_ptr(), &mut sfs) } != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "statfs() failed"))
        }
        Ok(sfs.to_fs())
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        let mut ifap: *mut ifaddrs = ptr::null_mut();
        if unsafe { getifaddrs(&mut ifap) } != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "getifaddrs() failed"))
        }
        let ifirst = ifap;
        let mut result = BTreeMap::new();
        while ifap != ptr::null_mut() {
            let ifa = unsafe { (*ifap) };
            let name = unsafe { ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned() };
            let mut entry = result.entry(name.clone()).or_insert(Network {
                name: name,
                addrs: Vec::new(),
            });
            let addr = parse_addr(ifa.ifa_addr);
            if addr != IpAddr::Unsupported {
                entry.addrs.push(NetworkAddrs {
                    addr: addr,
                    netmask: parse_addr(ifa.ifa_netmask),
                });
            }
            ifap = unsafe { (*ifap).ifa_next };
        }
        unsafe { freeifaddrs(ifirst) };
        Ok(result)
    }
}


fn parse_addr(aptr: *const sockaddr) -> IpAddr {
    if aptr == ptr::null() {
        return IpAddr::Empty;
    }
    let addr = unsafe { *aptr };
    match addr.sa_family as i32 {
        AF_INET => IpAddr::V4(Ipv4Addr::new(addr.sa_data[2] as u8, addr.sa_data[3] as u8,
                                            addr.sa_data[4] as u8, addr.sa_data[5] as u8)),
        AF_INET6 => {
            // This is horrible.
            let addr6: *const sockaddr_in6 = unsafe { mem::transmute(aptr) };
            let mut a: [u8; 16] = unsafe { (*addr6).sin6_addr.s6_addr };
            &mut a[..].reverse();
            let a: [u16; 8] = unsafe { mem::transmute(a) };
            IpAddr::V6(Ipv6Addr::new(a[7], a[6], a[5], a[4], a[3], a[2], a[1], a[0]))
        },
        _ => IpAddr::Unsupported,
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct sysctl_cpu {
    user: usize,
    nice: usize,
    system: usize,
    interrupt: usize,
    idle: usize,
}

impl<'a> Sub<&'a sysctl_cpu> for sysctl_cpu {
    type Output = sysctl_cpu;

    #[inline(always)]
    fn sub(self, rhs: &sysctl_cpu) -> sysctl_cpu {
        sysctl_cpu {
            user: self.user - rhs.user,
            nice: self.nice - rhs.nice,
            system: self.system - rhs.system,
            interrupt: self.interrupt - rhs.interrupt,
            idle: self.idle - rhs.idle,
        }
    }
}

impl sysctl_cpu {
    fn measure() -> io::Result<Vec<sysctl_cpu>> {
        let cpus = *CP_TIMES_SIZE / mem::size_of::<sysctl_cpu>();
        let mut data: Vec<sysctl_cpu> = Vec::with_capacity(cpus);
        unsafe { data.set_len(cpus) };
        sysctl!(KERN_CP_TIMES, &mut data[0], *CP_TIMES_SIZE);
        Ok(data)
    }

    fn to_cpuload(&self) -> CPULoad {
        let total = (self.user + self.nice + self.system + self.interrupt + self.idle) as f32;
        CPULoad {
            user: self.user as f32 / total,
            nice: self.nice as f32 / total,
            system: self.system as f32 / total,
            interrupt: self.interrupt as f32 / total,
            idle: self.idle as f32 / total,
        }
    }
}

#[repr(C)]
struct fsid_t {
    val: [i32; 2],
}

// FreeBSD's native struct. If you want to know what FreeBSD
// thinks about the POSIX statvfs struct, read man 3 statvfs :D
#[repr(C)]
struct statfs {
    f_version: u32,
    f_type: u32,
    f_flags: u64,
    f_bsize: u64,
    f_iosize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: i64,
    f_files: u64,
    f_ffree: i64,
    f_syncwrites: u64,
    f_asyncwrites: u64,
    f_syncreads: u64,
    f_asyncreads: u64,
    f_spare: [u64; 10],
    f_namemax: u32,
    f_owner: uid_t,
    f_fsid: fsid_t,
    f_charspare: [c_schar; 80],
    f_fstypename: [c_schar; 16],
    f_mntfromname: [c_schar; 88],
    f_mntonname: [c_schar; 88],
}

impl statfs {
    fn to_fs(&self) -> Filesystem {
        Filesystem {
            files: self.f_files as usize - self.f_ffree as usize,
            free: ByteSize::b(self.f_bfree as usize * self.f_bsize as usize),
            avail: ByteSize::b(self.f_bavail as usize * self.f_bsize as usize),
            total: ByteSize::b(self.f_blocks as usize * self.f_bsize as usize),
            name_max: self.f_namemax as usize,
            fs_type: unsafe { ffi::CStr::from_ptr(&self.f_fstypename[0]).to_string_lossy().into_owned() },
            fs_mounted_from: unsafe { ffi::CStr::from_ptr(&self.f_mntfromname[0]).to_string_lossy().into_owned() },
            fs_mounted_on: unsafe { ffi::CStr::from_ptr(&self.f_mntonname[0]).to_string_lossy().into_owned() },
        }
    }
}

#[link(name = "c")]
extern {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
    fn statfs(path: *const c_uchar, buf: *mut statfs) -> c_int;
    fn getpagesize() -> c_int;
}
