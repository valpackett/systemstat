use std::{io, path};
use std::collections::BTreeMap;
use data::*;

pub trait Platform {
    fn new() -> Self;
    fn cpu_load(&self) -> io::Result<Vec<CPULoad>>;
    fn load_average(&self) -> io::Result<LoadAverage>;
    fn mounts(&self) -> io::Result<Vec<Filesystem>>;
    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem>;
    fn networks(&self) -> io::Result<BTreeMap<String, Network>>;
}
