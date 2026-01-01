use super::common::*;
use super::unix;
use crate::data::*;
use crate::platform::procfs;
use std::{io, path};

pub struct PlatformImpl;

/// An implementation of `Platform` for Hurd.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>>{
        procfs::cpu_time().map(|times| {
            DelayedMeasurement::new(Box::new(move || {
                procfs::cpu_time().map(|delay_times| {
                    delay_times
                        .iter()
                        .zip(times.iter())
                        .map(|(now, prev)| (*now - prev).to_cpuload())
                        .collect::<Vec<_>>()
                })
            }))
        })
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        unix::load_average()
    }

    fn memory(&self) -> io::Result<Memory> {
        PlatformMemory::new().map(PlatformMemory::to_memory)
    }

    fn swap(&self) -> io::Result<Swap> {
        PlatformMemory::new().map(PlatformMemory::to_swap)
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        procfs::boot_time()
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        procfs::mounts()
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        procfs::mount_at(path)
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        procfs::block_device_statistics()
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, _interface: &str) -> io::Result<NetworkStats> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        procfs::socket_stats()
    }
}

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
        procfs::memory_stats()
            .map(|meminfo| PlatformMemory { meminfo })
    }

    // Convert the platform memory information to Memory
    fn to_memory(self) -> Memory {
        let meminfo = &self.meminfo;
        Memory {
            total: meminfo.get("MemTotal").copied().unwrap_or(ByteSize::b(0)),
            free: saturating_sub_bytes(
                meminfo.get("MemFree").copied().unwrap_or(ByteSize::b(0))
                    + meminfo.get("Buffers").copied().unwrap_or(ByteSize::b(0))
                    + meminfo.get("Cached").copied().unwrap_or(ByteSize::b(0))
                    + meminfo
                        .get("SReclaimable")
                        .copied()
                        .unwrap_or(ByteSize::b(0)),
                meminfo.get("Shmem").copied().unwrap_or(ByteSize::b(0)),
            ),
            platform_memory: self,
        }
    }

    // Convert the platform memory information to Swap
    fn to_swap(self) -> Swap {
        let meminfo = &self.meminfo;
        Swap {
            total: meminfo.get("SwapTotal").copied().unwrap_or(ByteSize::b(0)),
            free: meminfo.get("SwapFree").copied().unwrap_or(ByteSize::b(0)),
            platform_swap: self,
        }
    }
}
