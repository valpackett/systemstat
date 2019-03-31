use std::{io, path, mem, fs};
use std::io::Read;
use std::time::Duration;
use libc::{c_ulong, c_ushort, c_uint, c_long, c_schar, c_char};
use libc::statvfs;
use data::*;
use super::common::*;
use super::unix;
use nom::{digit, not_line_ending, space, is_space};
use std::str;
use std::path::Path;

fn read_file(path: &str) -> io::Result<String> {
    let mut s = String::new();
    fs::File::open(path)
        .and_then(|mut f| f.read_to_string(&mut s))
        .map(|_| s)
}

fn value_from_file<T: str::FromStr>(path: &str) -> io::Result<T> {
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
                io::Error::new(io::ErrorKind::InvalidData, err.to_string())
            },
        )
    })
}

/// Parse a `/proc/meminfo` line into (key, ByteSize)
named!(
    proc_meminfo_line<(String, ByteSize)>,
    complete!(do_parse!(
        key: flat_map!(take_until!(":"), parse_to!(String)) >>
        tag!(":") >>
        value: usize_s >>
        ws!(tag!("kB")) >>
        ((key, ByteSize::kib(value)))
    ))
);

/// Optionally parse a `/proc/meminfo` line`
named!(
    proc_meminfo_line_opt<Option<(String, ByteSize)>>,
    opt!(proc_meminfo_line)
);

/// Parse `/proc/meminfo` into a hashmap
named!(
    proc_meminfo<BTreeMap<String, ByteSize>>,
    fold_many0!(
        ws!(flat_map!(not_line_ending, proc_meminfo_line_opt)),
        BTreeMap::new(),
        |mut map: BTreeMap<String, ByteSize>, opt| {
            if let Some((key, val)) = opt {
                map.insert(key, val);
            }
            map
        }
    )
);

/// Get memory statistics
fn memory_stats() -> io::Result<BTreeMap<String, ByteSize>> {
    read_file("/proc/meminfo").and_then(|data| {
        proc_meminfo(data.as_bytes()).to_result().map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, err.to_string())
        })
    })
}

/// Parse a single word
named!(word_s<String>, flat_map!(
    map_res!(take_till!(is_space), str::from_utf8),
    parse_to!(String)
));

/// `/proc/mounts` data
struct ProcMountsData {
    source: String,
    target: String,
    fstype: String,
}

/// Parse a `/proc/mounts` line to get a mountpoint
named!(
    proc_mounts_line<ProcMountsData>,
    ws!(do_parse!(
        source: word_s >>
        target: word_s >>
        fstype: word_s >>
        (ProcMountsData {
            source: source,
            target: target,
            fstype: fstype,
        })
    ))
);

/// Parse `/proc/mounts` to get a list of mountpoints
named!(
    proc_mounts<Vec<ProcMountsData>>,
    many1!(ws!(flat_map!(not_line_ending, proc_mounts_line)))
);

/// `/proc/net/sockstat` data
struct ProcNetSockStat {
    tcp_in_use: usize,
    tcp_orphaned: usize,
    udp_in_use: usize,
}

/// Parse `/proc/net/sockstat` to get socket statistics
named!(
    proc_net_sockstat<ProcNetSockStat>,
    ws!(do_parse!(
        not_line_ending >>
        tag!("TCP: inuse") >>
        tcp_in_use: usize_s >>
        tag!("orphan") >>
        tcp_orphaned: usize_s >>
        not_line_ending >>
        tag!("UDP: inuse") >>
        udp_in_use: usize_s >>
        (ProcNetSockStat {
            tcp_in_use: tcp_in_use,
            tcp_orphaned: tcp_orphaned,
            udp_in_use: udp_in_use,
        })
    ))
);

/// `/proc/net/sockstat6` data
struct ProcNetSockStat6 {
    tcp_in_use: usize,
    udp_in_use: usize,
}

/// Parse `/proc/net/sockstat6` to get socket statistics
named!(
    proc_net_sockstat6<ProcNetSockStat6>,
    ws!(do_parse!(
        tag!("TCP6: inuse") >>
        tcp_in_use: usize_s >>
        tag!("UDP6: inuse") >>
        udp_in_use: usize_s >>
        (ProcNetSockStat6 {
            tcp_in_use: tcp_in_use,
            udp_in_use: udp_in_use,
        })
    ))
);

/// Stat a mountpoint to gather filesystem statistics
fn stat_mount(mount: ProcMountsData) -> io::Result<Filesystem> {
    let mut info: statvfs = unsafe { mem::zeroed() };
    let target = format!("{}\0", mount.target);
    let result = unsafe { statvfs(target.as_ptr() as *const c_char, &mut info) };
    match result {
        0 => Ok(Filesystem {
            files: info.f_files as usize - info.f_ffree as usize,
            files_total: info.f_files as usize,
            files_avail: info.f_favail as usize,
            free: ByteSize::b(info.f_bfree as usize * info.f_bsize as usize),
            avail: ByteSize::b(info.f_bavail as usize * info.f_bsize as usize),
            total: ByteSize::b(info.f_blocks as usize * info.f_bsize as usize),
            name_max: info.f_namemax as usize,
            fs_type: mount.fstype,
            fs_mounted_from: mount.source,
            fs_mounted_on: mount.target,
        }),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Parse a line of `/proc/diskstats`
named!(
    proc_diskstats_line<BlockDeviceStats>,
    ws!(do_parse!(
        major_number: usize_s >>
        minor_number: usize_s >>
        name: word_s >>
        read_ios: usize_s >>
        read_merges: usize_s >>
        read_sectors: usize_s >>
        read_ticks: usize_s >>
        write_ios: usize_s >>
        write_merges: usize_s >>
        write_sectors: usize_s >>
        write_ticks: usize_s >>
        in_flight: usize_s >>
        io_ticks: usize_s >>
        time_in_queue: usize_s >>
        (BlockDeviceStats {
            name: name,
            read_ios: read_ios,
            read_merges: read_merges,
            read_sectors: read_sectors,
            read_ticks: read_ticks,
            write_ios: write_ios,
            write_merges: write_merges,
            write_sectors: write_sectors,
            write_ticks: write_ticks,
            in_flight: in_flight,
            io_ticks: io_ticks,
            time_in_queue: time_in_queue
        })
    ))
);

/// Parse `/proc/diskstats` to get a Vec<BlockDeviceStats>
named!(proc_diskstats<Vec<BlockDeviceStats>>,
       many0!(ws!(flat_map!(not_line_ending, proc_diskstats_line)))
);


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
        memory_stats()
            .or_else(|_| {
                // If there's no procfs, e.g. in a chroot without mounting it or something
                let mut meminfo = BTreeMap::new();
                let mut info: sysinfo = unsafe { mem::zeroed() };
                unsafe { sysinfo(&mut info) };
                let unit = info.mem_unit as usize;
                meminfo.insert(
                    "MemTotal".to_owned(),
                    ByteSize::b(info.totalram as usize * unit),
                );
                meminfo.insert(
                    "MemFree".to_owned(),
                    ByteSize::b(info.freeram as usize * unit),
                );
                meminfo.insert(
                    "Shmem".to_owned(),
                    ByteSize::b(info.sharedram as usize * unit),
                );
                meminfo.insert(
                    "Buffers".to_owned(),
                    ByteSize::b(info.bufferram as usize * unit),
                );
                meminfo.insert(
                    "SwapTotal".to_owned(),
                    ByteSize::b(info.totalswap as usize * unit),
                );
                meminfo.insert(
                    "SwapFree".to_owned(),
                    ByteSize::b(info.freeswap as usize * unit),
                );
                Ok(meminfo)
            })
            .map(|meminfo| {
                Memory {
                    total: meminfo.get("MemTotal").map(|x| x.clone()).unwrap_or(
                        ByteSize::b(0),
                    ),
                    free: meminfo.get("MemFree").map(|x| x.clone()).unwrap_or(
                        ByteSize::b(0),
                    ) +
                        meminfo.get("Buffers").map(|x| x.clone()).unwrap_or(
                            ByteSize::b(0),
                        ) +
                        meminfo.get("Cached").map(|x| x.clone()).unwrap_or(
                            ByteSize::b(0),
                        ) +
                        meminfo.get("SReclaimable").map(|x| x.clone()).unwrap_or(
                            ByteSize::b(0),
                        ) -
                        meminfo.get("Shmem").map(|x| x.clone()).unwrap_or(
                            ByteSize::b(0),
                        ),
                    platform_memory: PlatformMemory { meminfo: meminfo },
                }
            })
    }

    fn uptime(&self) -> io::Result<Duration> {
        let mut info: sysinfo = unsafe { mem::zeroed() };
        unsafe { sysinfo(&mut info) };
        Ok(Duration::from_secs(info.uptime as u64))
    }

    fn boot_time(&self) -> io::Result<DateTime<Utc>> {
        read_file("/proc/stat").and_then(|data| {
            data.lines()
                .find(|line| line.starts_with("btime "))
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Could not find btime in /proc/stat"))
                .and_then(|line|
                  Utc.datetime_from_str(line, "btime %s")
                     .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string())))
        })
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
            if value_from_file::<String>(&(s.to_string() + "/type")).map(|t| t == "Battery").unwrap_or(false) {
                full += try!(
                    value_from_file::<i32>(&(s.to_string() + "/energy_full"))
                        .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_full")))
                );
                now += try!(
                    value_from_file::<i32>(&(s.to_string() + "/energy_now"))
                        .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_now")))
                );
                current += try!(
                    value_from_file::<i32>(&(s.to_string() + "/power_now"))
                        .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/current_now")))
                );
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
        let dir = "/sys/class/power_supply";
        let entries = try!(fs::read_dir(&dir));
        let mut on_ac = false;
        for e in entries {
            let p = e.unwrap().path();
            let s = p.to_str().unwrap();
            if value_from_file::<String>(&(s.to_string() + "/type")).map(|t| t == "Mains").unwrap_or(false) {
                on_ac |= try!(value_from_file::<i32>(&(s.to_string() + "/online")).map(|v| v == 1))
            }
        }
        Ok(on_ac)
    }

    fn mounts(&self) -> io::Result<Vec<Filesystem>> {
        read_file("/proc/mounts")
            .and_then(|data| {
                proc_mounts(data.as_bytes()).to_result().map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                })
            })
            .map(|mounts| {
                mounts
                    .into_iter()
                    .filter_map(|mount| stat_mount(mount).ok())
                    .collect()
            })
    }

    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem> {
        read_file("/proc/mounts")
            .and_then(|data| {
                proc_mounts(data.as_bytes()).to_result().map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                })
            })
            .and_then(|mounts| {
                mounts
                    .into_iter()
                    .find(|mount| Path::new(&mount.target) == path.as_ref())
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No such mount"))
            })
            .and_then(stat_mount)
    }

    fn block_device_statistics(&self) -> io::Result<BTreeMap<String, BlockDeviceStats>> {
        let mut result: BTreeMap<String, BlockDeviceStats> = BTreeMap::new();
        let stats: Vec<BlockDeviceStats> = try!(read_file("/proc/diskstats")
            .and_then(|data| {
                proc_diskstats(data.as_bytes()).to_result()
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
            })
        );

        for blkstats in stats {
            result.entry(blkstats.name.clone()).or_insert(blkstats);
        }
        Ok(result)
    }

    fn networks(&self) -> io::Result<BTreeMap<String, Network>> {
        unix::networks()
    }

    fn network_stats(&self, interface: &str) -> io::Result<NetworkStats> {
        let path_root: String = ("/sys/class/net/".to_string() + interface) + "/statistics/";
        let stats_file = |file: &str| (&path_root).to_string() + file;

        let rx_bytes: usize = try!(value_from_file::<usize>(&stats_file("rx_bytes")));
        let tx_bytes: usize = try!(value_from_file::<usize>(&stats_file("tx_bytes")));
        let rx_packets: usize = try!(value_from_file::<usize>(&stats_file("rx_packets")));
        let tx_packets: usize = try!(value_from_file::<usize>(&stats_file("tx_packets")));
        let rx_errors: usize = try!(value_from_file::<usize>(&stats_file("rx_errors")));
        let tx_errors: usize = try!(value_from_file::<usize>(&stats_file("tx_errors")));

        Ok(NetworkStats {
            rx_bytes,
            tx_bytes,
            rx_packets,
            tx_packets,
            rx_errors,
            tx_errors,
        })
    }

    fn cpu_temp(&self) -> io::Result<f32> {
        read_file("/sys/class/thermal/thermal_zone0/temp")
            .or(read_file("/sys/class/hwmon/hwmon0/temp1_input"))
            .and_then(|data| match data.trim().parse::<f32>() {
                Ok(x) => Ok(x),
                Err(_) => Err(io::Error::new(io::ErrorKind::Other, "Could not parse float")),
            })
            .map(|num| num / 1000.0)
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        let sockstats: ProcNetSockStat =
            try!(read_file("/proc/net/sockstat")
                 .and_then(|data| {
                     proc_net_sockstat(data.as_bytes()).to_result().map_err(|err| {
                         io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                     })
                 })
            );
        let sockstats6: ProcNetSockStat6 =
            try!(read_file("/proc/net/sockstat6")
                 .and_then(|data| {
                     proc_net_sockstat6(data.as_bytes()).to_result().map_err(|err| {
                         io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                     })
                 })
            );
        let result: SocketStats = SocketStats {
            tcp_sockets_in_use: sockstats.tcp_in_use,
            tcp_sockets_orphaned: sockstats.tcp_orphaned,
            udp_sockets_in_use: sockstats.udp_in_use,
            tcp6_sockets_in_use: sockstats6.tcp_in_use,
            udp6_sockets_in_use: sockstats6.udp_in_use,
        };
        Ok(result)
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
