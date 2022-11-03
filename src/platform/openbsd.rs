use std::{io, path, ptr, time, fs, mem, ffi, slice};
use std::os::unix::io::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::mem::size_of;
use libc::{c_void, c_int, c_uint, c_ulong, c_uchar, ioctl, sysctl, timeval, statfs, ifaddrs, getifaddrs, if_data, freeifaddrs};
use crate::data::*;
use super::common::*;
use super::unix;
use super::bsd;

pub struct PlatformImpl;

macro_rules! sysctl {
    ($mib:expr, $dataptr:expr, $size:expr, $shouldcheck:expr) => {
        {
            let mib = &$mib;
            let mut size = $size;
            if unsafe { sysctl(&mib[0], mib.len() as u32,
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
    static ref APM_IOC_GETPOWER: c_ulong = 0x40000000u64 | ((size_of::<apm_power_info>() & 0x1fff) << 16) as u64 | (0x41 << 8) | 3;
    // OpenBSD does not have sysctlnametomib, so more copy-pasting of magic numbers from C headers :(
    static ref HW_NCPU: [c_int; 2] = [6, 3];
    static ref KERN_CPTIME2: [c_int; 3] = [1, 71, 0];
    static ref KERN_BOOTTIME: [c_int; 2] = [1, 21];
    static ref VM_UVMEXP: [c_int; 2] = [2, 4];
    static ref VFS_BCACHESTAT: [c_int; 3] = [10, 0, 3];
}

#[link(name = "c")]
extern "C" {
    fn getmntinfo(mntbufp: *mut *mut statfs, flags: c_int) -> c_int;
}

/// An implementation of `Platform` for OpenBSD.
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
        PlatformMemory::new().map(|pm| pm.to_memory())
    }

    fn swap(&self) -> io::Result<Swap> {
        PlatformMemory::new().map(|pm| pm.to_swap())
    }

    fn memory_and_swap(&self) -> io::Result<(Memory, Swap)> {
        let pm = PlatformMemory::new()?;
        Ok((pm.clone().to_memory(), pm.to_swap()))
    }

    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        let mut data: timeval = unsafe { mem::zeroed() };
        sysctl!(KERN_BOOTTIME, &mut data, mem::size_of::<timeval>());
        let ts = OffsetDateTime::from_unix_timestamp(data.tv_sec.into()).expect("unix timestamp should be within range") + Duration::from_nanos(data.tv_usec as u64);
        Ok(ts)
    }

    // /dev/apm is probably the nicest interface I've seen :)
    fn battery_life(&self) -> io::Result<BatteryLife> {
        let f = fs::File::open("/dev/apm")?;
        let mut info = apm_power_info::default();
        if unsafe { ioctl(f.as_raw_fd(), *APM_IOC_GETPOWER, &mut info) } == -1 {
            return Err(io::Error::new(io::ErrorKind::Other, "ioctl() failed"))
        }
        if info.battery_state == 0xff { // APM_BATT_UNKNOWN
            return Err(io::Error::new(io::ErrorKind::Other, "Battery state unknown"))
        }
        if info.battery_state == 4 { // APM_BATTERY_ABSENT
            return Err(io::Error::new(io::ErrorKind::Other, "Battery absent"))
        }
        Ok(BatteryLife {
            remaining_capacity: info.battery_life as f32,
            remaining_time: time::Duration::from_secs(info.minutes_left as u64),
        })
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        let f = fs::File::open("/dev/apm")?;
        let mut info = apm_power_info::default();
        if unsafe { ioctl(f.as_raw_fd(), *APM_IOC_GETPOWER, &mut info) } == -1 {
            return Err(io::Error::new(io::ErrorKind::Other, "ioctl() failed"))
        }
        Ok(info.ac_state == 0x01) // APM_AC_ON
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

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
        let mut rx_bytes: u64   = 0;
        let mut tx_bytes: u64   = 0;
        let mut rx_packets: u64 = 0;
        let mut tx_packets: u64 = 0;
        let mut rx_errors: u64  = 0;
        let mut tx_errors: u64  = 0;
        let mut ifap: *mut ifaddrs = std::ptr::null_mut();
        let mut ifa: *mut ifaddrs;
        let mut data: *mut if_data;
        unsafe {
            getifaddrs(&mut ifap);
            ifa = ifap;
            // Multiple entries may be same network but for different addresses (ipv4, ipv6, link
            // layer)
            while !ifa.is_null() {
                let c_str: &std::ffi::CStr = std::ffi::CStr::from_ptr((*ifa).ifa_name);
                let str_net: &str = match c_str.to_str() {
                    Ok(v)  => v,
                    Err(_) => return Err(io::Error::new(io::ErrorKind::Other, "C string cannot be converted"))
                };
                if interface == str_net {
                    data        = (*ifa).ifa_data as *mut if_data;
                    // if_data may not be present in every table
                    if !data.is_null() {
                        rx_bytes   += (*data).ifi_ibytes;
                        tx_bytes   += (*data).ifi_obytes;
                        rx_packets += (*data).ifi_ipackets;
                        tx_packets += (*data).ifi_opackets;
                        rx_errors  += (*data).ifi_ierrors;
                        tx_errors  += (*data).ifi_oerrors;
                    }
                }
                ifa = (*ifa).ifa_next;
            }
            freeifaddrs(ifap);
        }
        Ok(NetworkStats {
            rx_bytes: ByteSize::b(rx_bytes),
            tx_bytes: ByteSize::b(tx_bytes),
            rx_packets,
            tx_packets,
            rx_errors,
            tx_errors,
        })
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }
}

fn measure_cpu() -> io::Result<Vec<CpuTime>> {
    let mut cpus: usize = 0;
    sysctl!(HW_NCPU, &mut cpus, mem::size_of::<usize>());
    let mut data: Vec<sysctl_cpu> = Vec::with_capacity(cpus);
    unsafe { data.set_len(cpus) };
    for i in 0..cpus {
        let mut mib = KERN_CPTIME2.clone();
        mib[2] = i as i32;
        sysctl!(mib, &mut data[i], mem::size_of::<sysctl_cpu>());
    }
    Ok(data.into_iter().map(|cpu| cpu.into()).collect())
}

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
        let mut uvm_info = uvmexp::default();
        sysctl!(VM_UVMEXP, &mut uvm_info, mem::size_of::<uvmexp>());
        let mut bcache_info = bcachestats::default();
        sysctl!(
            VFS_BCACHESTAT,
            &mut bcache_info,
            mem::size_of::<bcachestats>()
        );

        Ok(Self {
            total: ByteSize::kib((uvm_info.npages << *bsd::PAGESHIFT) as u64),
            active: ByteSize::kib((uvm_info.active << *bsd::PAGESHIFT) as u64),
            inactive: ByteSize::kib((uvm_info.inactive << *bsd::PAGESHIFT) as u64),
            wired: ByteSize::kib((uvm_info.wired << *bsd::PAGESHIFT) as u64),
            cache: ByteSize::kib((bcache_info.numbufpages << *bsd::PAGESHIFT) as u64),
            free: ByteSize::kib((uvm_info.free << *bsd::PAGESHIFT) as u64),
            paging: ByteSize::kib((uvm_info.paging << *bsd::PAGESHIFT) as u64),
            sw: ByteSize::kib((uvm_info.swpages << *bsd::PAGESHIFT) as u64),
            swinuse: ByteSize::kib((uvm_info.swpginuse << *bsd::PAGESHIFT) as u64),
            swonly: ByteSize::kib((uvm_info.swpgonly << *bsd::PAGESHIFT) as u64),
        })
    }
    fn to_memory(self) -> Memory {
        Memory {
            total: self.total,
            free: self.inactive + self.cache + self.free + self.paging,
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

fn statfs_to_fs(fs: &statfs) -> Filesystem {
    Filesystem {
        files: (fs.f_files as usize).saturating_sub(fs.f_ffree as usize),
        files_total: fs.f_files as usize,
        files_avail: fs.f_ffree as usize,
        free: ByteSize::b(fs.f_bfree * fs.f_bsize as u64),
        avail: ByteSize::b(fs.f_bavail as u64 * fs.f_bsize as u64),
        total: ByteSize::b(fs.f_blocks * fs.f_bsize as u64),
        name_max: fs.f_namemax as usize,
        fs_type: unsafe { ffi::CStr::from_ptr(&fs.f_fstypename[0]).to_string_lossy().into_owned() },
        fs_mounted_from: unsafe { ffi::CStr::from_ptr(&fs.f_mntfromname[0]).to_string_lossy().into_owned() },
        fs_mounted_on: unsafe { ffi::CStr::from_ptr(&fs.f_mntonname[0]).to_string_lossy().into_owned() },
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Fields of KERN_CPTIME2: https://github.com/openbsd/src/blob/0403d5bcc6af6e3b8d03ad1c6de319d5acc58295/sys/sys/sched.h#L83
pub struct sysctl_cpu {
    user: usize,
    nice: usize,
    system: usize,
    spin: usize,
    interrupt: usize,
    idle: usize,
}

impl From<sysctl_cpu> for CpuTime {
    fn from(cpu: sysctl_cpu) -> CpuTime {
        CpuTime {
            user: cpu.user,
            nice: cpu.nice,
            system: cpu.system,
            interrupt: cpu.interrupt,
            idle: cpu.idle,
            other: 0,
        }
    }
}

#[derive(Default, Debug)]
#[repr(C)]
struct apm_power_info {
    battery_state: c_uchar,
    ac_state: c_uchar,
    battery_life: c_uchar,
    spare1: c_uchar,
    minutes_left: c_uint,
    spare2: [c_uint; 6],
}

#[derive(Default, Debug)]
#[repr(C)]
struct bcachestats {
    numbufs: i64,		/* number of buffers allocated */
    numbufpages: i64,		/* number of pages in buffer cache */
    numdirtypages: i64, 		/* number of dirty free pages */
    numcleanpages: i64, 		/* number of clean free pages */
    pendingwrites: i64,		/* number of pending writes */
    pendingreads: i64,		/* number of pending reads */
    numwrites: i64,		/* total writes started */
    numreads: i64,		/* total reads started */
    cachehits: i64,		/* total reads found in cache */
    busymapped: i64,		/* number of busy and mapped buffers */
    dmapages: i64,		/* dma reachable pages in buffer cache */
    highpages: i64,		/* pages above dma region */
    delwribufs: i64,		/* delayed write buffers */
    kvaslots: i64,		/* kva slots total */
    kvaslots_avail: i64,		/* available kva slots */
    highflips: i64,		/* total flips to above DMA */
    highflops: i64,		/* total failed flips to above DMA */
    dmaflips: i64,		/* total flips from high to DMA */
}

#[derive(Default, Debug)]
#[repr(C)]
struct uvmexp {
    /* vm_page constants */
    pagesize: c_int,   /* size of a page (PAGE_SIZE): must be power of 2 */
    pagemask: c_int,   /* page mask */
    pageshift: c_int,  /* page shift */

    /* vm_page counters */
    npages: c_int,     /* number of pages we manage */
    free: c_int,       /* number of free pages */
    active: c_int,     /* number of active pages */
    inactive: c_int,   /* number of pages that we free'd but may want back */
    paging: c_int,	/* number of pages in the process of being paged out */
    wired: c_int,      /* number of wired pages */

    zeropages: c_int,		/* number of zero'd pages */
    reserve_pagedaemon: c_int, /* number of pages reserved for pagedaemon */
    reserve_kernel: c_int,	/* number of pages reserved for kernel */
    anonpages: c_int,		/* number of pages used by anon pagers */
    vnodepages: c_int,		/* number of pages used by vnode page cache */
    vtextpages: c_int,		/* number of pages used by vtext vnodes */

    /* pageout params */
    freemin: c_int,    /* min number of free pages */
    freetarg: c_int,   /* target number of free pages */
    inactarg: c_int,   /* target number of inactive pages */
    wiredmax: c_int,   /* max number of wired pages */
    anonmin: c_int,	/* min threshold for anon pages */
    vtextmin: c_int,	/* min threshold for vtext pages */
    vnodemin: c_int,	/* min threshold for vnode pages */
    anonminpct: c_int,	/* min percent anon pages */
    vtextminpct: c_int,/* min percent vtext pages */
    vnodeminpct: c_int,/* min percent vnode pages */

    /* swap */
    nswapdev: c_int,	/* number of configured swap devices in system */
    swpages: c_int,	/* number of PAGE_SIZE'ed swap pages */
    swpginuse: c_int,	/* number of swap pages in use */
    swpgonly: c_int,	/* number of swap pages in use, not also in RAM */
    nswget: c_int,	/* number of times fault calls uvm_swap_get() */
    nanon: c_int,	/* number total of anon's in system */
    nanonneeded: c_int,/* number of anons currently needed */
    nfreeanon: c_int,	/* number of free anon's */

    /* stat counters */
    faults: c_int,		/* page fault count */
    traps: c_int,		/* trap count */
    intrs: c_int,		/* interrupt count */
    swtch: c_int,		/* context switch count */
    softs: c_int,		/* software interrupt count */
    syscalls: c_int,		/* system calls */
    pageins: c_int,		/* pagein operation count */
    /* pageouts are in pdpageouts below */
    obsolete_swapins: c_int,	/* swapins */
    obsolete_swapouts: c_int,	/* swapouts */
    pgswapin: c_int,		/* pages swapped in */
    pgswapout: c_int,		/* pages swapped out */
    forks: c_int,  		/* forks */
    forks_ppwait: c_int,	/* forks where parent waits */
    forks_sharevm: c_int,	/* forks where vmspace is shared */
    pga_zerohit: c_int,	/* pagealloc where zero wanted and zero
                           was available */
    pga_zeromiss: c_int,	/* pagealloc where zero wanted and zero
                               not available */
    zeroaborts: c_int,		/* number of times page zeroing was
                               aborted */

    /* fault subcounters */
    fltnoram: c_int,	/* number of times fault was out of ram */
    fltnoanon: c_int,	/* number of times fault was out of anons */
    fltnoamap: c_int,	/* number of times fault was out of amap chunks */
    fltpgwait: c_int,	/* number of times fault had to wait on a page */
    fltpgrele: c_int,	/* number of times fault found a released page */
    fltrelck: c_int,	/* number of times fault relock called */
    fltrelckok: c_int,	/* number of times fault relock is a success */
    fltanget: c_int,	/* number of times fault gets anon page */
    fltanretry: c_int,	/* number of times fault retrys an anon get */
    fltamcopy: c_int,	/* number of times fault clears "needs copy" */
    fltnamap: c_int,	/* number of times fault maps a neighbor anon page */
    fltnomap: c_int,	/* number of times fault maps a neighbor obj page */
    fltlget: c_int,	/* number of times fault does a locked pgo_get */
    fltget: c_int,	/* number of times fault does an unlocked get */
    flt_anon: c_int,	/* number of times fault anon (case 1a) */
    flt_acow: c_int,	/* number of times fault anon cow (case 1b) */
    flt_obj: c_int,	/* number of times fault is on object page (2a) */
    flt_prcopy: c_int,	/* number of times fault promotes with copy (2b) */
    flt_przero: c_int,	/* number of times fault promotes with zerofill (2b) */

    /* daemon counters */
    pdwoke: c_int,	/* number of times daemon woke up */
    pdrevs: c_int,	/* number of times daemon rev'd clock hand */
    pdswout: c_int,	/* number of times daemon called for swapout */
    pdfreed: c_int,	/* number of pages daemon freed since boot */
    pdscans: c_int,	/* number of pages daemon scanned since boot */
    pdanscan: c_int,	/* number of anonymous pages scanned by daemon */
    pdobscan: c_int,	/* number of object pages scanned by daemon */
    pdreact: c_int,	/* number of pages daemon reactivated since boot */
    pdbusy: c_int,	/* number of times daemon found a busy page */
    pdpageouts: c_int,	/* number of times daemon started a pageout */
    pdpending: c_int,	/* number of times daemon got a pending pagout */
    pddeact: c_int,	/* number of pages daemon deactivates */
    pdreanon: c_int,	/* anon pages reactivated due to min threshold */
    pdrevnode: c_int,	/* vnode pages reactivated due to min threshold */
    pdrevtext: c_int,	/* vtext pages reactivated due to min threshold */

    fpswtch: c_int,	/* FPU context switches */
    kmapent: c_int,	/* number of kernel map entries */
}

