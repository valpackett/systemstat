use std::{io, path, ptr, mem, ffi, slice};
use libc::{c_int, c_schar, c_uchar, uid_t};
use data::*;
use super::common::*;

pub struct PlatformImpl;

impl Platform for PlatformImpl {
    fn new() -> Self {
        PlatformImpl
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
