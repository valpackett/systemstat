// You are likely to be eaten by a grue.

use std::{io, path, ptr, mem, ffi, slice};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::collections::BTreeMap;
use libc::{c_void, c_int, c_schar, c_uchar, size_t, uid_t, sysctl, sysctlnametomib,
           getifaddrs, freeifaddrs, ifaddrs, sockaddr, sockaddr_in6, AF_INET, AF_INET6};
use data::*;
use super::common::*;

pub struct PlatformImpl;

lazy_static! {
    static ref KERN_CP_TIMES: [c_int; 2] = {
        let mut mib = [0, 0];
        let mut sz: size_t = mib.len();
        let s = ffi::CString::new("kern.cp_times").unwrap();
        unsafe { sysctlnametomib(s.as_ptr(), &mut mib[0], &mut sz) };
        mib
    };

    static ref CP_TIMES_SIZE: usize = {
        let mut size: usize = 0;
        unsafe { sysctl(&KERN_CP_TIMES[0], KERN_CP_TIMES.len() as u32,
                        ptr::null_mut(), &mut size, ptr::null(), 0) };
        size
    };
}

impl Platform for PlatformImpl {
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<Vec<CPULoad>> {
        let mut size = *CP_TIMES_SIZE;
        let cpus = size / mem::size_of::<sysctl_cpu>();
        let mut data: Vec<sysctl_cpu> = Vec::with_capacity(cpus);
        unsafe { data.set_len(cpus) };
        if unsafe { sysctl(&KERN_CP_TIMES[0], KERN_CP_TIMES.len() as u32,
                           &mut data[0] as *mut _ as *mut c_void, &mut size, ptr::null(), 0) } != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "sysctl() failed"))
        }
        Ok(data.iter().map(|c| c.to_cpuload()).collect::<Vec<_>>())
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
struct sysctl_cpu {
    user: usize,
    nice: usize,
    system: usize,
    interrupt: usize,
    idle: usize,
}

impl sysctl_cpu {
    fn to_cpuload(&self) -> CPULoad {
        let total = (self.user + self.nice + self.system + self.interrupt + self.idle) as f32;
        CPULoad {
            user_percent: self.user as f32 / total,
            nice_percent: self.nice as f32 / total,
            system_percent: self.system as f32 / total,
            interrupt_percent: self.interrupt as f32 / total,
            idle_percent: self.idle as f32 / total,
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
            files: self.f_files - self.f_ffree as u64,
            free_bytes: self.f_bfree as u64 * self.f_bsize,
            avail_bytes: self.f_bavail as u64 * self.f_bsize,
            total_bytes: self.f_blocks as u64 * self.f_bsize,
            name_max: self.f_namemax as u64,
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
}
