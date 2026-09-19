use super::common::*;
use super::unix;
use crate::data::*;
use crate::platform::procfs;
use crate::reader_utils::read_file;
use libc::{c_long, c_schar, c_uint, c_ulong, c_ushort};
use std::cell::RefCell;
use std::io::Read;
use std::path::Path;
use std::str;
use std::time::Duration;
use std::{fs, io, mem, path};

pub fn value_from_file<T: str::FromStr>(path: &str) -> io::Result<T> {
    read_file(path)?
        .trim_end_matches('\n')
        .parse()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("File: \"{}\" doesn't contain an int value", &path),
            )
        })
}

fn capacity(charge_full: i32, charge_now: i32) -> f32 {
    charge_now as f32 / charge_full as f32
}

fn time(on_ac: bool, charge_full: i32, charge_now: i32, current_now: i32) -> Duration {
    if current_now != 0 {
        if on_ac {
            // Charge time
            Duration::from_secs(
                charge_full.saturating_sub(charge_now).abs() as u64 * 3600u64 / current_now as u64,
            )
        } else {
            // Discharge time
            Duration::from_secs(charge_now as u64 * 3600u64 / current_now as u64)
        }
    } else {
        Duration::new(0, 0)
    }
}

// we only care about tdie temp, rather than being able to read individual core temp
//  if we want core temps, we should search "coretemp", "k8temp", "k10temp"
const DRIVERS: [&str; 6] = [
    // Intel
    "x86_pkg_temp",
    // AMD
    "k10temp",
    "zenpower",
    // Qualcomm
    "cpuss0-thermal",
    "cpuss-0-0-thermal",
    "cpuss0-top-thermal",
];

fn find_cpu_temp_path() -> io::Result<String> {
    try_search_path("/sys/class/thermal/", "/type", "/temp").or(try_search_path(
        "/sys/class/hwmon/",
        "/name",
        "/temp1_input",
    ))
}

fn try_search_path(search_dir: &str, driver_file: &str, temp_file: &str) -> io::Result<String> {
    for entry in fs::read_dir(search_dir)? {
        let Ok(dir_entry) = entry else { continue };
        let Some(dir) = dir_entry.file_name().into_string().ok() else {
            continue;
        };

        let base_path = search_dir.to_string() + dir.as_str();
        if let Ok(driver) = read_file((base_path.clone() + driver_file).as_str()) {
            if DRIVERS.contains(&driver.as_str().trim()) {
                return Ok(base_path + temp_file);
            }
        }
    }

    Err(io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("No compatible CPU temp drivers found in {}", search_dir),
    ))
}

pub struct PlatformImpl {
    cpu_temp_file: RefCell<Option<fs::File>>,
}

/// An implementation of `Platform` for Linux.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl {
            cpu_temp_file: RefCell::new(None),
        }
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
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

    fn memory_and_swap(&self) -> io::Result<(Memory, Swap)> {
        let pm = PlatformMemory::new()?;
        Ok((pm.clone().to_memory(), pm.to_swap()))
    }

    fn uptime(&self) -> io::Result<Duration> {
        let mut info: sysinfo = unsafe { mem::zeroed() };
        unsafe { sysinfo(&mut info) };
        Ok(Duration::from_secs(info.uptime as u64))
    }

    fn boot_time(&self) -> io::Result<OffsetDateTime> {
        procfs::boot_time()
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        let dir = "/sys/class/power_supply";
        let entries = fs::read_dir(&dir)?;
        let mut full = 0;
        let mut now = 0;
        let mut current = 0;
        for e in entries {
            let p = e.unwrap().path();
            let s = p.to_str().unwrap();
            if value_from_file::<String>(&(s.to_string() + "/type"))
                .map(|t| t == "Battery")
                .unwrap_or(false)
            {
                let f = value_from_file::<i32>(&(s.to_string() + "/energy_full"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_full")));
                let n = value_from_file::<i32>(&(s.to_string() + "/energy_now"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_now")));
                let c = value_from_file::<i32>(&(s.to_string() + "/power_now"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/current_now")));
                if let (Ok(f), Ok(n), Ok(c)) = (f, n, c) {
                    full += f;
                    now += n;
                    current += c;
                }
            }
        }
        if full != 0 {
            let on_ac = matches!(self.on_ac_power(), Ok(true));
            Ok(BatteryLife {
                remaining_capacity: capacity(full, now),
                remaining_time: time(on_ac, full, now, current),
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Missing battery information",
            ))
        }
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        let dir = "/sys/class/power_supply";
        let entries = fs::read_dir(&dir)?;
        let mut on_ac = false;
        for e in entries {
            let p = e.unwrap().path();
            let s = p.to_str().unwrap();
            if value_from_file::<String>(&(s.to_string() + "/type"))
                .map(|t| t == "Mains")
                .unwrap_or(false)
            {
                on_ac |= value_from_file::<i32>(&(s.to_string() + "/online")).map(|v| v == 1)?
            }
        }
        Ok(on_ac)
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

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
        let path_root: String = ("/sys/class/net/".to_string() + interface) + "/statistics/";
        let stats_file = |file: &str| (&path_root).to_string() + file;

        let rx_bytes: u64 = value_from_file::<u64>(&stats_file("rx_bytes"))?;
        let tx_bytes: u64 = value_from_file::<u64>(&stats_file("tx_bytes"))?;
        let rx_packets: u64 = value_from_file::<u64>(&stats_file("rx_packets"))?;
        let tx_packets: u64 = value_from_file::<u64>(&stats_file("tx_packets"))?;
        let rx_errors: u64 = value_from_file::<u64>(&stats_file("rx_errors"))?;
        let tx_errors: u64 = value_from_file::<u64>(&stats_file("tx_errors"))?;

        Ok(NetworkStats {
            rx_bytes: ByteSize::b(rx_bytes),
            tx_bytes: ByteSize::b(tx_bytes),
            rx_packets,
            tx_packets,
            rx_errors,
            tx_errors,
        })
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        let mut cpu_temp_file = self.cpu_temp_file.borrow_mut();
        if cpu_temp_file.is_none() {
            let cpu_temp_path = find_cpu_temp_path()?;
            *cpu_temp_file = Some(fs::File::open(Path::new(&cpu_temp_path))?);
        }

        let mut data = String::new();
        match cpu_temp_file.as_ref().unwrap().read_to_string(&mut data) {
            Ok(_) => {}
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Could not read cpu temp file",
                ))
            }
        }
        let value = match data.trim().parse::<f32>() {
            Ok(x) => x,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Could not parse float",
                ))
            }
        };

        Ok(value / 1000.0)
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        procfs::socket_stats()
    }
}

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
        procfs::memory_stats()
            .or_else(|_| {
                // If there's no procfs, e.g. in a chroot without mounting it or something
                let mut meminfo = BTreeMap::new();
                let mut info: sysinfo = unsafe { mem::zeroed() };
                unsafe { sysinfo(&mut info) };
                let unit = info.mem_unit as u64;
                meminfo.insert(
                    "MemTotal".to_owned(),
                    ByteSize::b(info.totalram as u64 * unit),
                );
                meminfo.insert(
                    "MemFree".to_owned(),
                    ByteSize::b(info.freeram as u64 * unit),
                );
                meminfo.insert(
                    "Shmem".to_owned(),
                    ByteSize::b(info.sharedram as u64 * unit),
                );
                meminfo.insert(
                    "Buffers".to_owned(),
                    ByteSize::b(info.bufferram as u64 * unit),
                );
                meminfo.insert(
                    "SwapTotal".to_owned(),
                    ByteSize::b(info.totalswap as u64 * unit),
                );
                meminfo.insert(
                    "SwapFree".to_owned(),
                    ByteSize::b(info.freeswap as u64 * unit),
                );
                Ok(meminfo)
            })
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

#[repr(C)]
#[derive(Debug)]
struct sysinfo {
    uptime: c_long,
    loads: [c_ulong; 3],
    totalram: c_ulong,
    freeram: c_ulong,
    sharedram: c_ulong,
    bufferram: c_ulong,
    totalswap: c_ulong,
    freeswap: c_ulong,
    procs: c_ushort,
    totalhigh: c_ulong,
    freehigh: c_ulong,
    mem_unit: c_uint,
    padding: [c_schar; 8],
}

#[link(name = "c")]
extern "C" {
    fn sysinfo(info: *mut sysinfo);
}
