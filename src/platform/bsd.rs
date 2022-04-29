use libc::c_int;
use crate::data::*;

lazy_static! {
    pub static ref PAGESHIFT: c_int = {
        let mut pagesize = unsafe { getpagesize() };
        let mut pageshift = 0;
        while pagesize > 1 {
            pageshift += 1;
            pagesize >>= 1;
        }
        pageshift - 10 // LOG1024
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct sysctl_cpu {
    user: usize,
    nice: usize,
    system: usize,
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

#[link(name = "c")]
extern "C" {
    fn getpagesize() -> c_int;
}
