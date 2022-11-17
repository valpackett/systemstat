use std::{io, path, convert::{TryFrom, TryInto}};
use crate::data::*;

/// The Platform trait declares all the functions for getting system information.
///
/// NOTE: any impl MUST override one of `uptime` or `boot_time`.
pub trait Platform {
    fn new() -> Self;

    /// Returns a delayed vector of CPU load statistics, one object per CPU (core).
    ///
    /// You need to wait some time (about a second is good) before unwrapping the
    /// `DelayedMeasurement` with `.done()`.
    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>>;

    /// Returns a delayed CPU load statistics object, average over all CPUs (cores).
    ///
    /// You need to wait some time (about a second is good) before unwrapping the
    /// `DelayedMeasurement` with `.done()`.
    fn cpu_load_aggregate(&self) -> io::Result<DelayedMeasurement<CPULoad>> {
        let measurement = self.cpu_load()?;
        Ok(DelayedMeasurement::new(
                Box::new(move || measurement.done().map(|ls| {
                    let mut it = ls.iter();
                    let first = it.next().unwrap().clone(); // has to be a variable, rust moves the iterator otherwise
                    it.fold(first, |acc, l| acc.avg_add(l))
                }))))
    }

    /// Returns a vector of CPU time statistics, one object per CPU (core).
    fn cpu_time(&self) -> io::Result<Vec<CpuTime>>;

    /// Returns a vector of CPU time statistics, sum over all CPUs (cores).
    fn cpu_time_aggregate(&self) -> io::Result<CpuTime> {
        let measurement = self.cpu_time()?;
        Ok(measurement.into_iter().fold(
            CpuTime {
                user: 0,
                nice: 0,
                system: 0,
                interrupt: 0,
                idle: 0,
                other: 0,
            },
            |acc, x| CpuTime {
                user: acc.user + x.user,
                nice: acc.nice + x.nice,
                system: acc.system + x.system,
                interrupt: acc.interrupt + x.interrupt,
                idle: acc.idle + x.idle,
                other: acc.other + x.other,
            },
        ))
    }

    /// Returns a load average object.
    fn load_average(&self) -> io::Result<LoadAverage>;

    /// Returns a memory information object.
    fn memory(&self) -> io::Result<Memory>;

    /// Returns a swap memory information object.
    fn swap(&self) -> io::Result<Swap>;

    /// Returns a swap and a memory information object.
    /// On some platforms this is more efficient than calling memory() and swap() separately
    /// If memory() or swap() are not implemented for a platform, this function will fail.
    fn memory_and_swap(&self) -> io::Result<(Memory, Swap)> {
        // Do swap first, in order to fail fast if it's not implemented
        let swap = self.swap()?;
        let memory = self.memory()?;
        Ok((memory, swap))
    }

    /// Returns the system uptime.
    fn uptime(&self) -> io::Result<Duration> {
        self.boot_time().and_then(|bt| {
            (OffsetDateTime::now_utc() - bt)
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Could not process time"))
        })
    }

    /// Returns the system boot time.
    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        self.uptime().and_then(|ut| {
            Ok(OffsetDateTime::now_utc()
                - time::Duration::try_from(ut)
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "Could not process time"))?)
        })
    }

    /// Returns a battery life information object.
    fn battery_life(&self) -> io::Result<BatteryLife>;

    /// Returns whether AC power is plugged in.
    fn on_ac_power(&self) -> io::Result<bool>;

    /// Returns a vector of filesystem mount information objects.
    fn mounts(&self) -> io::Result<Vec<Filesystem>>;

    /// Returns a filesystem mount information object for the filesystem at a given path.
    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        self.mounts()
            .and_then(|mounts| {
                mounts
                    .into_iter()
                    .find(|mount| path::Path::new(&mount.fs_mounted_on) == path.as_ref())
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No such mount"))
        })
    }

    /// Returns a map of block device statistics objects
    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>>;

    /// Returns a map of network intefrace information objects.
    ///
    /// It's a map because most operating systems return an object per IP address, not per
    /// interface, and we're doing deduplication and packing everything into one object per
    /// interface. You can use the .values() iterator if you need to iterate over all of them.
    fn networks(&self) -> io::Result<BTreeMap<String, Network>>;

    /// Returns statistics for a given interface (bytes/packets sent/received)
    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats>;

    /// Returns the current CPU temperature in degrees Celsius.
    ///
    /// Depending on the platform, this might be core 0, package, etc.
    fn cpu_temp(&self) -> io::Result<f32>;

    /// Returns information about the number of sockets in use
    fn socket_stats(&self) -> io::Result<SocketStats>;
}
