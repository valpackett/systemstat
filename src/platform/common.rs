use std::{io, path};
use std::collections::BTreeMap;
use data::*;

pub trait Platform {
    fn new() -> Self;

    fn cpu_load(&self) -> io::Result<Vec<CPULoad>>;

    fn cpu_load_aggregate(&self) -> io::Result<CPULoad> {
        self.cpu_load().map(|ls| {
            let mut it = ls.iter();
            let first = it.next().unwrap().clone(); // has to be a variable, rust moves the iterator otherwise
            it.fold(first, |acc, l| acc + l)
        })
    }

    fn load_average(&self) -> io::Result<LoadAverage>;

    fn mounts(&self) -> io::Result<Vec<Filesystem>>;

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem>;

    fn networks(&self) -> io::Result<BTreeMap<String, Network>>;
}
