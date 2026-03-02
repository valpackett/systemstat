use super::common::*;
use super::unix;
use crate::data::*;
use std::{io, path};

pub struct PlatformImpl;

fn named_u64(named: &[kstat_rs::Named], key: &str) -> usize {
    for n in named {
        if n.name == key {
            if let kstat_rs::NamedData::UInt64(v) = n.value {
                return v as usize;
            }
        }
    }
    0
}

fn measure_cpu() -> io::Result<Vec<CpuTime>> {
    let ctl = kstat_rs::Ctl::new()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut result = Vec::new();
    for mut ks in ctl.filter(Some("cpu"), None, Some("sys")) {
        let instance = ks.ks_instance;
        let data = ctl.read(&mut ks)
            .map_err(|e| io::Error::other(e.to_string()))?;
        if let kstat_rs::Data::Named(named) = data {
            result.push((instance, CpuTime {
                user: named_u64(&named, "cpu_ticks_user"),
                nice: 0,
                system: named_u64(&named, "cpu_ticks_kernel"),
                interrupt: 0,
                idle: named_u64(&named, "cpu_ticks_idle"),
                other: named_u64(&named, "cpu_ticks_wait"),
            }));
        }
    }
    result.sort_by_key(|(instance, _)| *instance);
    Ok(result.into_iter().map(|(_, cpu)| cpu).collect())
}

/// An implementation of `Platform` for illumos.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        let loads = measure_cpu()?;
        Ok(DelayedMeasurement::new(
                Box::new(move || Ok(loads.iter()
                               .zip(measure_cpu()?.iter())
                               .map(|(prev, now)| (*now - prev).to_cpuload())
                               .collect::<Vec<_>>()))))
    }

    fn load_average(&self) -> io::Result<LoadAverage> {
        unix::load_average()
    }

    fn memory(&self) -> io::Result<Memory> {
        Err(io::Error::other("Not supported"))
    }

    fn swap(&self) -> io::Result<Swap> {
        Err(io::Error::other("Not supported"))
    }

    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        let ctl = kstat_rs::Ctl::new()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let mut ks = ctl.filter(Some("unix"), Some(0), Some("system_misc"))
            .next()
            .ok_or_else(|| io::Error::other("kstat unix:0:system_misc not found"))?;
        let data = ctl.read(&mut ks)
            .map_err(|e| io::Error::other(e.to_string()))?;
        if let kstat_rs::Data::Named(named) = data {
            for n in &named {
                if n.name == "boot_time" {
                    if let kstat_rs::NamedData::UInt32(v) = n.value {
                        return OffsetDateTime::from_unix_timestamp(v as i64)
                            .map_err(|e| io::Error::other(e.to_string()));
                    }
                }
            }
        }
        Err(io::Error::other("boot_time not found in kstat"))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        Err(io::Error::other("Not supported"))
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        Err(io::Error::other("Not supported"))
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        Err(io::Error::other("Not supported"))
    }

    fn mount_at<P: AsRef<path::Path>>(&self, _: P) -> io::Result<Filesystem> {
        Err(io::Error::other("Not supported"))
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        Err(io::Error::other("Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, _interface: &str) -> io::Result<NetworkStats> {
        Err(io::Error::other("Not supported"))
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        Err(io::Error::other("Not supported"))
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        Err(io::Error::other("Not supported"))
    }
}
