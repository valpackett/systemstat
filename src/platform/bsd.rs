use std::io;
use libc::c_int;
use std::ops::Sub;
use data::*;

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
    pub fn to_cpuload(&self) -> CPULoad {
        let mut total = (self.user + self.nice + self.system + self.interrupt + self.idle) as f32;
        if total < 0.00001 {
            total = 0.00001;
        }
        CPULoad {
            user: self.user as f32 / total,
            nice: self.nice as f32 / total,
            system: self.system as f32 / total,
            interrupt: self.interrupt as f32 / total,
            idle: self.idle as f32 / total,
        }
    }
}

pub fn load_average() -> io::Result<LoadAverage> {
    let mut loads: [f64; 3] = [0.0, 0.0, 0.0];
    if unsafe { getloadavg(&mut loads[0], 3) } != 3 {
        return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"))
    }
    Ok(LoadAverage {
        one: loads[0] as f32, five: loads[1] as f32, fifteen: loads[2] as f32
    })
}

#[link(name = "c")]
extern "C" {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
    fn getpagesize() -> c_int;
}
