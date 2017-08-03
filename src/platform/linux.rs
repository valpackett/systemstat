use std::{io, path, mem, fs};
use std::io::Read;
use std::time::Duration;
use libc::{c_ulong, c_ushort, c_uint, c_long, c_schar};
use data::*;
use super::common::*;
use super::unix;
use nom::{digit, not_line_ending, space};
use std::str;

fn read_file(path: &str) -> io::Result<String> {
    let mut s = String::new();
    fs::File::open(path)
        .and_then(|mut f| f.read_to_string(&mut s))
        .map(|_| s)
}

fn value_from_file(path: &str) -> io::Result<i32> {
    try!(read_file(path))
        .trim_right_matches("\n")
        .parse()
        .and_then(|n| Ok(n))
        .or_else(|_| {
            Err(io::Error::new(io::ErrorKind::Other,
                               format!("File: \"{}\" doesn't contain an int value", &path)))
        })
}

fn capacity(charge_full: i32, charge_now: i32) -> f32 {
    charge_now as f32 / charge_full as f32
}

fn time(on_ac: bool, charge_full: i32, charge_now: i32, current_now: i32) -> Duration {
    if current_now != 0 {
        if on_ac {
            // Charge time
            Duration::from_secs((charge_full - charge_now).abs() as u64 * 3600u64 / current_now as u64)
        }
        else {
            // Discharge time
            Duration::from_secs(charge_now as u64 * 3600u64 / current_now as u64)
        }
    } else {
        Duration::new(0, 0)
    }
}

/// Parse an unsigned integer out of a string, surrounded by whitespace
named!(
    usize_s<usize>,
    ws!(map_res!(
        map_res!(digit, str::from_utf8),
        str::FromStr::from_str
    ))
);

/// Parse `cpuX`, where X is a number
named!(proc_stat_cpu_prefix<()>, do_parse!(tag!("cpu") >> digit >> ()));

/// Parse a `/proc/stat` CPU line into a `CpuTime` struct
named!(
    proc_stat_cpu_time<CpuTime>,
    do_parse!(
        ws!(proc_stat_cpu_prefix) >>
        user: usize_s >>
        nice: usize_s >>
        system: usize_s >>
        idle: usize_s >>
        iowait: usize_s >>
        irq: usize_s >>
            (CpuTime {
                 user: user,
                 nice: nice,
                 system: system,
                 idle: idle,
                 interrupt: irq,
                 other: iowait,
             })
    )
);

/// Parse the top CPU load aggregate line of `/proc/stat`
named!(proc_stat_cpu_aggregate<()>, do_parse!(tag!("cpu") >> space >> ()));

/// Parse `/proc/stat` to extract per-CPU loads
named!(
    proc_stat_cpu_times<Vec<CpuTime>>,
    do_parse!(
        ws!(flat_map!(not_line_ending, proc_stat_cpu_aggregate)) >>
        result: many1!(ws!(flat_map!(not_line_ending, proc_stat_cpu_time))) >>
        (result)
    )
);

/// Get the current per-CPU `CpuTime` statistics
fn cpu_time() -> io::Result<Vec<CpuTime>> {
    read_file("/proc/stat").and_then(|data| {
        proc_stat_cpu_times(data.as_bytes()).to_result().map_err(
            |err| {
                io::Error::new(io::ErrorKind::InvalidData, err)
            },
        )
    })
}

pub struct PlatformImpl;

/// An implementation of `Platform` for Linux.
/// See `Platform` for documentation.
impl Platform for PlatformImpl {
    #[inline(always)]
    fn new() -> Self {
        PlatformImpl
    }

    fn cpu_load(&self) -> io::Result<DelayedMeasurement<Vec<CPULoad>>> {
        cpu_time().map(|times| {
            DelayedMeasurement::new(Box::new(move || {
                cpu_time().map(|delay_times| {
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
        let mut meminfo = BTreeMap::new();
        if let Ok(meminfo_str) = read_file("/proc/meminfo") {
            for line in meminfo_str.lines() {
                if let Some(colon_idx) = line.find(':') {
                    let (name, val) = line.split_at(colon_idx);
                    if let Ok(size) = val.trim().trim_left_matches(':').trim().trim_right_matches(char::is_alphabetic).trim().parse::<usize>() {
                        meminfo.insert(name.to_owned(), ByteSize::kib(size));
                    }
                }
            }
        } else { // If there's no procfs, e.g. in a chroot without mounting it or something
            let mut info: sysinfo = unsafe { mem::zeroed() };
            unsafe { sysinfo(&mut info) };
            let unit = info.mem_unit as usize;
            meminfo.insert("MemTotal".to_owned(), ByteSize::b(info.totalram as usize * unit));
            meminfo.insert("MemFree".to_owned(), ByteSize::b(info.freeram as usize * unit));
            meminfo.insert("Shmem".to_owned(), ByteSize::b(info.sharedram as usize * unit));
            meminfo.insert("Buffers".to_owned(), ByteSize::b(info.bufferram as usize * unit));
        };
        Ok(Memory {
            total: meminfo.get("MemTotal").map(|x| x.clone()).unwrap_or(ByteSize::b(0)),
            free: meminfo.get("MemFree").map(|x| x.clone()).unwrap_or(ByteSize::b(0))
                + meminfo.get("Buffers").map(|x| x.clone()).unwrap_or(ByteSize::b(0))
                + meminfo.get("Cached").map(|x| x.clone()).unwrap_or(ByteSize::b(0))
                + meminfo.get("SReclaimable").map(|x| x.clone()).unwrap_or(ByteSize::b(0))
                - meminfo.get("Shmem").map(|x| x.clone()).unwrap_or(ByteSize::b(0)),
            platform_memory: PlatformMemory { meminfo: meminfo },
        })
    }

    fn uptime(&self) -> io::Result<Duration> {
        let mut info: sysinfo = unsafe { mem::zeroed() };
        unsafe { sysinfo(&mut info) };
        Ok(Duration::from_secs(info.uptime as u64))
    }

    fn battery_life(&self) -> io::Result<BatteryLife> {
        let dir = "/sys/class/power_supply";
        let entries = try!(fs::read_dir(&dir));
        let mut full = 0;
        let mut now = 0;
        let mut current = 0;
        for e in entries {
            let p = e.unwrap().path();
            let s = p.to_str().unwrap();
            let f = p.file_name().unwrap().to_str().unwrap();
            if f.len() > 3 {
                if f.split_at(3).0 == "BAT" {
                    full += try!(value_from_file(&(s.to_string() + "/energy_full"))
                                 .or_else(|_| value_from_file(&(s.to_string() + "/charge_full"))));
                    now += try!(value_from_file(&(s.to_string() + "/energy_now"))
                                .or_else(|_| value_from_file(&(s.to_string() + "/charge_now"))));
                    current += try!(value_from_file(&(s.to_string() + "/power_now"))
                                    .or_else(|_| value_from_file(&(s.to_string() + "/current_now"))));
                }
            }
        }
        if full != 0 {
            let on_ac =
                match self.on_ac_power() {
                    Ok(true) => true,
                    _ => false,
                };
            Ok(BatteryLife {
                remaining_capacity: capacity(full, now),
                remaining_time: time(on_ac, full, now, current),
            })
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Missing battery information"))
        }
    }

    fn on_ac_power(&self) -> io::Result<bool> {
        value_from_file("/sys/class/power_supply/AC/online").map(|v| v == 1)
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        Err(io::Error::new(io::ErrorKind::Other, "Not supported"))
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
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
