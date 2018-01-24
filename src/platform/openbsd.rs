use std::{io, path, ptr, time, fs, mem};
use std::os::unix::io::AsRawFd;
use std::mem::size_of;
use libc::{c_void, c_int, c_uint, c_ulong, c_uchar, ioctl, sysctl, timeval};
use data::*;
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
    static ref APM_IOC_GETPOWER: c_ulong = (0x40000000u64 | ((size_of::<apm_power_info>() & 0x1fff) << 16) as u64 | (0x41 << 8) | 3);
    // OpenBSD does not have sysctlnametomib, so more copy-pasting of magic numbers from C headers :(
    static ref HW_NCPU: [c_int; 2] = [6, 3];
    static ref KERN_CPTIME2: [c_int; 3] = [1, 71, 0];
    static ref KERN_BOOTTIME: [c_int; 2] = [1, 21];
    static ref VM_UVMEXP: [c_int; 2] = [2, 4];
    static ref VFS_BCACHESTAT: [c_int; 3] = [10, 0, 3];
}

/// An implementation of `Platform` for OpenBSD.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        let loads = try!(measure_cpu());
        Ok(DelayedMeasurement::new(
                Box::new(move || Ok(loads.iter()
                               .zip(try!(measure_cpu()).iter())
                               .map(|(prev, now)| (*now - prev).to_cpuload())
                               .collect::<Vec<_>>()))))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        unix::load_average()
    }

    fn memory(&self) -> io::Result<Memory> {
        let mut uvm_info = uvmexp::default(); sysctl!(VM_UVMEXP, &mut uvm_info, mem::size_of::<uvmexp>());
        let mut bcache_info = bcachestats::default(); sysctl!(VFS_BCACHESTAT, &mut bcache_info, mem::size_of::<bcachestats>());
        let total = ByteSize::kib((uvm_info.npages << *bsd::PAGESHIFT) as usize);
        let pmem = PlatformMemory {
            active: ByteSize::kib((uvm_info.active << *bsd::PAGESHIFT) as usize),
            inactive: ByteSize::kib((uvm_info.inactive << *bsd::PAGESHIFT) as usize),
            wired: ByteSize::kib((uvm_info.wired << *bsd::PAGESHIFT) as usize),
            cache: ByteSize::kib((bcache_info.numbufpages << *bsd::PAGESHIFT) as usize),
            free: ByteSize::kib((uvm_info.free << *bsd::PAGESHIFT) as usize),
            paging: ByteSize::kib((uvm_info.paging << *bsd::PAGESHIFT) as usize),
        };
        Ok(Memory {
            total: total,
            free: pmem.inactive + pmem.cache + pmem.free + pmem.paging,
            platform_memory: pmem,
        })
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        let mut data: timeval = unsafe { mem::zeroed() };
        sysctl!(KERN_BOOTTIME, &mut data, mem::size_of::<timeval>());
        Ok(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(data.tv_sec, data.tv_usec as u32), Utc))
    }

    // /dev/apm is probably the nicest interface I've seen :)
    fn battery_life(&self) -> io::Result<BatteryLife> {
        let f = try!(fs::File::open("/dev/apm"));
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
        let f = try!(fs::File::open("/dev/apm"));
        let mut info = apm_power_info::default();
        if unsafe { ioctl(f.as_raw_fd(), *APM_IOC_GETPOWER, &mut info) } == -1 {
            return Err(io::Error::new(io::ErrorKind::Other, "ioctl() failed"))
        }
        Ok(info.ac_state == 0x01) // APM_AC_ON
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mount_at<P: AsRef<path::Path>>(&self, _: P) -> io::Result<Filesystem> {
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
}

fn measure_cpu() -> io::Result<Vec<CpuTime>> {
    let mut cpus: usize = 0;
    sysctl!(HW_NCPU, &mut cpus, mem::size_of::<usize>());
    let mut data: Vec<bsd::sysctl_cpu> = Vec::with_capacity(cpus);
    unsafe { data.set_len(cpus) };
    for i in 0..cpus {
        let mut mib = KERN_CPTIME2.clone();
        mib[2] = i as i32;
        sysctl!(mib, &mut data[i], mem::size_of::<bsd::sysctl_cpu>());
    }
    Ok(data.into_iter().map(|cpu| cpu.into()).collect())
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
