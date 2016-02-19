use std::io;
use data::*;

pub trait Platform {
    fn load_average(&self) -> io::Result<LoadAverage>;
}
