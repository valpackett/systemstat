// use super::bsd;
use super::common::*;
use super::unix;
use crate::data::*;
use libc::{c_int, c_void, sysctl, CTL_VM};
use std::{io, mem, path, ptr};

pub struct PlatformImpl;

// https://github.com/NetBSD/src/blob/8e2e7cb174ca27b848b18119f33cf4c212fe22ee/sys/uvm/uvm_param.h#L169
static VM_UVMEXP2: c_int = 5;

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

/// An implementation of `Platform` for NetBSD.
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
        PlatformMemory::new().map(|pm| pm.to_memory())
    }

    fn swap(&self) -> io::Result<Swap> {
        PlatformMemory::new().map(|pm| pm.to_swap())
    }

    fn memory_and_swap(&self) -> io::Result<(Memory, Swap)> {
        let pm = PlatformMemory::new()?;
        Ok((pm.clone().to_memory(), pm.to_swap()))
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
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

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
        let mut uvm_info = uvmexp_sysctl::default();
        sysctl!(
            &[CTL_VM, VM_UVMEXP2],
            &mut uvm_info,
            mem::size_of::<uvmexp_sysctl>()
        );

        Ok(Self {
            pageshift: uvm_info.pageshift,
            total: ByteSize::b((uvm_info.npages << uvm_info.pageshift) as u64),
            active: ByteSize::b((uvm_info.active << uvm_info.pageshift) as u64),
            inactive: ByteSize::b((uvm_info.inactive << uvm_info.pageshift) as u64),
            wired: ByteSize::b((uvm_info.wired << uvm_info.pageshift) as u64),
            anon: ByteSize::b((uvm_info.anonpages << uvm_info.pageshift) as u64),
            files: ByteSize::b((uvm_info.filepages << uvm_info.pageshift) as u64),
            exec: ByteSize::b((uvm_info.execpages << uvm_info.pageshift) as u64),
            free: ByteSize::b((uvm_info.free << uvm_info.pageshift) as u64),
            paging: ByteSize::b((uvm_info.paging << uvm_info.pageshift) as u64),
            sw: ByteSize::b((uvm_info.swpages << uvm_info.pageshift) as u64),
            swinuse: ByteSize::b((uvm_info.swpginuse << uvm_info.pageshift) as u64),
            swonly: ByteSize::b((uvm_info.swpgonly << uvm_info.pageshift) as u64),
        })
    }
    fn to_memory(self) -> Memory {
        Memory {
            total: self.total,
            free: self.free,
            platform_memory: self,
        }
    }
    fn to_swap(self) -> Swap {
        Swap {
            total: self.sw,
            free: saturating_sub_bytes(self.sw, self.swinuse),
            platform_swap: self,
        }
    }
}

// https://github.com/NetBSD/src/blob/038135cba4b80f5c8d1e32fbc5b73c91c2f276d9/sys/uvm/uvm_extern.h#L420-L515
#[repr(C)]
#[derive(Debug, Default)]
struct uvmexp_sysctl {
    pagesize: i64,
    pagemask: i64,
    pageshift: i64,
    npages: i64,
    free: i64,
    active: i64,
    inactive: i64,
    paging: i64,
    wired: i64,
    zeropages: i64,
    reserve_pagedaemon: i64,
    reserve_kernel: i64,
    freemin: i64,
    freetarg: i64,
    inactarg: i64, // unused
    wiredmax: i64,
    nswapdev: i64,
    swpages: i64,
    swpginuse: i64,
    swpgonly: i64,
    nswget: i64,
    unused1: i64, // unused; was nanon
    cpuhit: i64,
    cpumiss: i64,
    faults: i64,
    traps: i64,
    intrs: i64,
    swtch: i64,
    softs: i64,
    syscalls: i64,
    pageins: i64,
    swapins: i64,  // unused
    swapouts: i64, // unused
    pgswapin: i64, // unused
    pgswapout: i64,
    forks: i64,
    forks_ppwait: i64,
    forks_sharevm: i64,
    pga_zerohit: i64,
    pga_zeromiss: i64,
    zeroaborts: i64,
    fltnoram: i64,
    fltnoanon: i64,
    fltpgwait: i64,
    fltpgrele: i64,
    fltrelck: i64,
    fltrelckok: i64,
    fltanget: i64,
    fltanretry: i64,
    fltamcopy: i64,
    fltnamap: i64,
    fltnomap: i64,
    fltlget: i64,
    fltget: i64,
    flt_anon: i64,
    flt_acow: i64,
    flt_obj: i64,
    flt_prcopy: i64,
    flt_przero: i64,
    pdwoke: i64,
    pdrevs: i64,
    unused4: i64,
    pdfreed: i64,
    pdscans: i64,
    pdanscan: i64,
    pdobscan: i64,
    pdreact: i64,
    pdbusy: i64,
    pdpageouts: i64,
    pdpending: i64,
    pddeact: i64,
    anonpages: i64,
    filepages: i64,
    execpages: i64,
    colorhit: i64,
    colormiss: i64,
    ncolors: i64,
    bootpages: i64,
    poolpages: i64,
    countsyncone: i64,
    countsyncall: i64,
    anonunknown: i64,
    anonclean: i64,
    anondirty: i64,
    fileunknown: i64,
    fileclean: i64,
    filedirty: i64,
    fltup: i64,
    fltnoup: i64,
}
