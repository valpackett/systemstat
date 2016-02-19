use std::io;
use libc::{c_int};
use data::*;
use super::common::*;

pub struct PlatformImpl;

impl Platform for PlatformImpl {
    fn load_average(&self) -> io::Result<LoadAverage> {
        let mut loads: [f64; 3] = [0.0, 0.0, 0.0];
        if unsafe { getloadavg(&mut loads[0], 3) } != 3 {
            return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"))
        }
        Ok(LoadAverage {
            one: loads[0] as f32, five: loads[1] as f32, fifteen: loads[2] as f32
        })
    }
}

#[link(name = "c")]
extern {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
}
