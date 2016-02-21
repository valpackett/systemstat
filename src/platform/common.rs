use std::{io, path};
use std::collections::BTreeMap;
use data::*;

/// The Platform trait declares all the functions for getting system information.
pub trait Platform {
    fn new() -> Self;

    /// Returns a vector of CPU load statistics, one object per CPU (core).
    fn cpu_load(&self) -> io::Result<Vec<CPULoad>>;

    /// Returns a CPU load statistics object, average over all CPUs (cores).
    fn cpu_load_aggregate(&self) -> io::Result<CPULoad> {
        self.cpu_load().map(|ls| {
            let mut it = ls.iter();
            let first = it.next().unwrap().clone(); // has to be a variable, rust moves the iterator otherwise
            it.fold(first, |acc, l| acc + l)
        })
    }

    /// Returns a load average object.
    fn load_average(&self) -> io::Result<LoadAverage>;

    /// Returns a vector of filesystem mount information objects.
    fn mounts(&self) -> io::Result<Vec<Filesystem>>;

    /// Returns a filesystem mount information object for the filesystem at a given path.
    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem>;

    /// Returns a map of network intefrace information objects.
    ///
    /// It's a map because most operating systems return an object per IP address, not per
    /// interface, and we're doing deduplication and packing everything into one object per
    /// interface. You can use the .values() iterator if you need to iterate over all of them.
    fn networks(&self) -> io::Result<BTreeMap<String, Network>>;
}
