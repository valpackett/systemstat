use super::common::*;
use super::unix;
use crate::data::*;
use libc::statvfs;
use libc::{c_char, c_long, c_schar, c_uint, c_ulong, c_ushort};
use nom::bytes::complete::{tag, take_till, take_until};
use nom::character::complete::{digit1, multispace0, not_line_ending, space1};
use nom::character::is_space;
use nom::combinator::{complete, map, map_res, opt, verify};
use nom::error::ParseError;
use nom::multi::{fold_many0, many0, many1};
use nom::sequence::{delimited, preceded, tuple};
use nom::{IResult, Parser};
use std::io::Read;
use std::path::Path;
use std::str;
use std::time::Duration;
use std::{fs, io, mem, path};

fn read_file(path: &str) -> io::Result<String> {
    let mut s = String::new();
    fs::File::open(path)
        .and_then(|mut f| f.read_to_string(&mut s))
        .map(|_| s)
}

fn value_from_file<T: str::FromStr>(path: &str) -> io::Result<T> {
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

/// A combinator that takes a parser `inner` and produces a parser that also consumes both leading and
/// trailing whitespace, returning the output of `inner`.
fn ws<'a, F: 'a, O, E: ParseError<&'a str>>(
    inner: F,
) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    delimited(multispace0, inner, multispace0)
}

/// Parse an unsigned integer out of a string, surrounded by whitespace
fn usize_s(input: &str) -> IResult<&str, usize> {
    map_res(
        map_res(map(ws(digit1), str::as_bytes), str::from_utf8),
        str::FromStr::from_str,
    )(input)
}

// Parse `cpuX`, where X is a number
fn proc_stat_cpu_prefix(input: &str) -> IResult<&str, ()> {
    map(tuple((tag("cpu"), digit1)), |_| ())(input)
}

// Parse a `/proc/stat` CPU line into a `CpuTime` struct
fn proc_stat_cpu_time(input: &str) -> IResult<&str, CpuTime> {
    map(
        preceded(
            ws(proc_stat_cpu_prefix),
            tuple((usize_s, usize_s, usize_s, usize_s, usize_s, usize_s)),
        ),
        |(user, nice, system, idle, iowait, irq)| CpuTime {
            user,
            nice,
            system,
            idle,
            interrupt: irq,
            other: iowait,
        },
    )(input)
}

// Parse the top CPU load aggregate line of `/proc/stat`
fn proc_stat_cpu_aggregate(input: &str) -> IResult<&str, ()> {
    map(tuple((tag("cpu"), space1)), |_| ())(input)
}

// Parse `/proc/stat` to extract per-CPU loads
fn proc_stat_cpu_times(input: &str) -> IResult<&str, Vec<CpuTime>> {
    preceded(
        map(ws(not_line_ending), proc_stat_cpu_aggregate),
        many1(map_res(ws(not_line_ending), |input| {
            proc_stat_cpu_time(input)
                .map(|(_, res)| res)
                .map_err(|_| ())
        })),
    )(input)
}

#[test]
fn test_proc_stat_cpu_times() {
    let input = "cpu  5972658 30964 2383250 392840200 70075 0 43945 0 0 0
cpu0 444919 3155 198700 24405593 4622 0 36738 0 0 0
cpu1 296558 428 76249 24715635 1426 0 1280 0 0 0
cpu2 402963 949 231689 24417433 6386 0 1780 0 0 0
cpu3 301571 2452 88088 24698799 1906 0 177 0 0 0
cpu4 427192 2896 200043 24427598 4640 0 519 0 0 0
cpu5 301433 515 86228 24695368 3925 0 107 0 0 0
cpu6 432794 2884 202838 24426726 4213 0 380 0 0 0
cpu7 304364 337 89802 24709831 2965 0 78 0 0 0
cpu8 475829 3608 211253 24379789 5645 0 438 0 0 0
cpu9 306784 880 86744 24704036 4669 0 81 0 0 0
cpu10 444170 3768 212504 24415053 5346 0 331 0 0 0
cpu11 300957 519 87052 24712048 4294 0 77 0 0 0
cpu12 445953 3608 209153 24415924 5458 0 288 0 0 0
cpu13 318262 752 89195 24681010 4133 0 1254 0 0 0
cpu14 451390 3802 216997 24404205 4852 0 283 0 0 0
cpu15 317509 401 96705 24631145 5588 0 124 0 0 0
intr 313606509 40 27 0 0 0 0 0 58 1 94578 0 2120 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 31 170440 151744 109054 197097 174402 169253 171292 0 0 0 1251812 0 0 0 0 0 0 0 0 6302 0 0 0 0 0 0 0 58 0 0 0 0 0 916279 10132 140390 8096 69021 79664 26669 79961 34865 33195 102807 124189 76108 69587 7073 3 9710 116522 10436256 0 2079496 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
ctxt 535905166
btime 1605203377
processes 1360293
procs_running 1
procs_blocked 0
softirq 81473629 1251347 8827732 10 325789 37 0 177903 43807896 2777 27080138
";
    let result = proc_stat_cpu_times(input).unwrap().1;
    assert_eq!(result.len(), 16);
    assert_eq!(result[0].user, 444919);
    assert_eq!(result[0].nice, 3155);
    assert_eq!(result[0].system, 198700);
    assert_eq!(result[0].idle, 24405593);
    assert_eq!(result[0].other, 4622);
    assert_eq!(result[0].interrupt, 0);
}

/// Get the current per-CPU `CpuTime` statistics
fn cpu_time() -> io::Result<Vec<CpuTime>> {
    read_file("/proc/stat").and_then(|data| {
        proc_stat_cpu_times(&data)
            .map(|(_, res)| res)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
    })
}

// Parse a `/proc/meminfo` line into (key, ByteSize)
fn proc_meminfo_line(input: &str) -> IResult<&str, (&str, ByteSize)> {
    complete(map(
        tuple((take_until(":"), delimited(tag(":"), usize_s, ws(tag("kB"))))),
        |(key, value)| (key, ByteSize::kib(value as u64)),
    ))(input)
}

// Optionally parse a `/proc/meminfo` line`
fn proc_meminfo_line_opt(input: &str) -> IResult<&str, Option<(&str, ByteSize)>> {
    opt(proc_meminfo_line)(input)
}

// Parse `/proc/meminfo` into a hashmap
fn proc_meminfo(input: &str) -> IResult<&str, BTreeMap<String, ByteSize>> {
    fold_many0(
        map_res(
            verify(ws(not_line_ending), |item: &str| !item.is_empty()),
            |input| {
                proc_meminfo_line_opt(input)
                    .map(|(_, res)| res)
                    .map_err(|_| ())
            },
        ),
        BTreeMap::new,
        |mut map: BTreeMap<String, ByteSize>, opt| {
            if let Some((key, val)) = opt {
                map.insert(key.to_string(), val);
            }
            map
        },
    )(input)
}

#[test]
fn test_proc_meminfo() {
    let input = "MemTotal:       32345596 kB
MemFree:        13160208 kB
MemAvailable:   27792164 kB
Buffers:            4724 kB
Cached:         14776312 kB
SwapCached:            0 kB
Active:          8530160 kB
Inactive:        9572028 kB
Active(anon):      18960 kB
Inactive(anon):  3415400 kB
Active(file):    8511200 kB
Inactive(file):  6156628 kB
Unevictable:           0 kB
Mlocked:               0 kB
SwapTotal:       6143996 kB
SwapFree:        6143996 kB
Dirty:             66124 kB
Writeback:             0 kB
AnonPages:       3313376 kB
Mapped:           931060 kB
Shmem:            134716 kB
KReclaimable:     427080 kB
Slab:             648316 kB
SReclaimable:     427080 kB
SUnreclaim:       221236 kB
KernelStack:       18752 kB
PageTables:        30576 kB
NFS_Unstable:          0 kB
Bounce:                0 kB
WritebackTmp:          0 kB
CommitLimit:    22316792 kB
Committed_AS:    7944504 kB
VmallocTotal:   34359738367 kB
VmallocUsed:       78600 kB
VmallocChunk:          0 kB
Percpu:            10496 kB
HardwareCorrupted:     0 kB
AnonHugePages:         0 kB
ShmemHugePages:        0 kB
ShmemPmdMapped:        0 kB
FileHugePages:         0 kB
FilePmdMapped:         0 kB
HugePages_Total:       0
HugePages_Free:        0
HugePages_Rsvd:        0
HugePages_Surp:        0
Hugepagesize:       2048 kB
Hugetlb:               0 kB
DirectMap4k:     1696884 kB
DirectMap2M:    17616896 kB
DirectMap1G:    13631488 kB
";
    let result = proc_meminfo(input).unwrap().1;
    assert_eq!(result.len(), 47);
    assert_eq!(
        result.get(&"Buffers".to_string()),
        Some(&ByteSize::kib(4724))
    );
    assert_eq!(
        result.get(&"KReclaimable".to_string()),
        Some(&ByteSize::kib(427080))
    );
}

/// Get memory statistics
fn memory_stats() -> io::Result<BTreeMap<String, ByteSize>> {
    read_file("/proc/meminfo").and_then(|data| {
        proc_meminfo(&data)
            .map(|(_, res)| res)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
    })
}

// Parse a single word
fn word_s(input: &str) -> IResult<&str, &str> {
    take_till(|c| is_space(c as u8))(input)
}

/// `/proc/mounts` data
struct ProcMountsData {
    source: String,
    target: String,
    fstype: String,
}

// Parse a `/proc/mounts` line to get a mountpoint
fn proc_mounts_line(input: &str) -> IResult<&str, ProcMountsData> {
    map(
        tuple((ws(word_s), ws(word_s), ws(word_s))),
        |(source, target, fstype)| ProcMountsData {
            source: source.to_string(),
            target: target.to_string(),
            fstype: fstype.to_string(),
        },
    )(input)
}

// Parse `/proc/mounts` to get a list of mountpoints
fn proc_mounts(input: &str) -> IResult<&str, Vec<ProcMountsData>> {
    many1(map_res(ws(not_line_ending), |input| {
        if input.is_empty() {
            Err(())
        } else {
            proc_mounts_line(input).map(|(_, res)| res).map_err(|_| ())
        }
    }))(input)
}

#[test]
fn test_proc_mounts() {
    let test_input_1 = r#"/dev/md0 / btrfs rw,noatime,space_cache,subvolid=15192,subvol=/var/lib/docker/btrfs/subvolumes/df6eb8d3ce1a295bcc252e51ba086cb7705a046a79a342b74729f3f738129f04 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
tmpfs /dev tmpfs rw,nosuid,size=65536k,mode=755,inode64 0 0
devpts /dev/pts devpts rw,nosuid,noexec,relatime,gid=5,mode=620,ptmxmode=666 0 0
sysfs /sys sysfs ro,nosuid,nodev,noexec,relatime 0 0
tmpfs /sys/fs/cgroup tmpfs rw,nosuid,nodev,noexec,relatime,mode=755,inode64 0 0"#;
    let mounts = proc_mounts(test_input_1).unwrap().1;
    assert!(mounts.len() == 6);
    let root = mounts.iter().find(|m| m.target == "/").unwrap();
    assert!(root.source == "/dev/md0");
    assert!(root.target == "/");
    assert!(root.fstype == "btrfs");

    let test_input_2 = r#"proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
tmpfs /dev tmpfs rw,nosuid,size=65536k,mode=755,inode64 0 0
devpts /dev/pts devpts rw,nosuid,noexec,relatime,gid=5,mode=620,ptmxmode=666 0 0
sysfs /sys sysfs ro,nosuid,nodev,noexec,relatime 0 0
tmpfs /sys/fs/cgroup tmpfs rw,nosuid,nodev,noexec,relatime,mode=755,inode64 0 0
/dev/md0 / btrfs rw,noatime,space_cache,subvolid=15192,subvol=/var/lib/docker/btrfs/subvolumes/df6eb8d3ce1a295bcc252e51ba086cb7705a046a79a342b74729f3f738129f04 0 0"#;
    let mounts = proc_mounts(test_input_2).unwrap().1;
    assert!(mounts.len() == 6);
    let root = mounts.iter().find(|m| m.target == "/").unwrap();
    assert!(root.source == "/dev/md0");
    assert!(root.target == "/");
    assert!(root.fstype == "btrfs");

    // On some distros, there is a blank line at the end of `/proc/mounts`,
    // so we test here to make sure we do not crash on that
    let test_input_3 = r#"proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
sys /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
dev /dev devtmpfs rw,nosuid,relatime,size=16131864k,nr_inodes=4032966,mode=755,inode64 0 0
run /run tmpfs rw,nosuid,nodev,relatime,mode=755,inode64 0 0
efivarfs /sys/firmware/efi/efivars efivarfs rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p3 / btrfs rw,noatime,ssd,space_cache,subvolid=5,subvol=/ 0 0
securityfs /sys/kernel/security securityfs rw,nosuid,nodev,noexec,relatime 0 0
tmpfs /dev/shm tmpfs rw,nosuid,nodev,inode64 0 0
"#;
    let mounts = proc_mounts(test_input_3).unwrap().1;
    assert!(mounts.len() == 8);
    let root = mounts.iter().find(|m| m.target == "/").unwrap();
    assert!(root.source == "/dev/nvme0n1p3");
    assert!(root.target == "/");
    assert!(root.fstype == "btrfs");
}

/// `/proc/net/sockstat` data
struct ProcNetSockStat {
    tcp_in_use: usize,
    tcp_orphaned: usize,
    udp_in_use: usize,
}

// Parse `/proc/net/sockstat` to get socket statistics
fn proc_net_sockstat(input: &str) -> IResult<&str, ProcNetSockStat> {
    map(
        preceded(
            not_line_ending,
            tuple((
                preceded(ws(tag("TCP: inuse")), usize_s),
                delimited(ws(tag("orphan")), usize_s, not_line_ending),
                preceded(ws(tag("UDP: inuse")), usize_s),
            )),
        ),
        |(tcp_in_use, tcp_orphaned, udp_in_use)| ProcNetSockStat {
            tcp_in_use,
            tcp_orphaned,
            udp_in_use,
        },
    )(input)
}

#[test]
fn test_proc_net_sockstat() {
    let input = "sockets: used 925
TCP: inuse 20 orphan 0 tw 12 alloc 23 mem 2
UDP: inuse 1 mem 2
UDPLITE: inuse 0
RAW: inuse 0
FRAG: inuse 0 memory 0
";
    let result = proc_net_sockstat(input).unwrap().1;
    assert_eq!(result.tcp_in_use, 20);
    assert_eq!(result.tcp_orphaned, 0);
    assert_eq!(result.udp_in_use, 1);
}

/// `/proc/net/sockstat6` data
struct ProcNetSockStat6 {
    tcp_in_use: usize,
    udp_in_use: usize,
}

// Parse `/proc/net/sockstat6` to get socket statistics
fn proc_net_sockstat6(input: &str) -> IResult<&str, ProcNetSockStat6> {
    map(
        ws(tuple((
            preceded(tag("TCP6: inuse"), usize_s),
            preceded(tag("UDP6: inuse"), usize_s),
        ))),
        |(tcp_in_use, udp_in_use)| ProcNetSockStat6 {
            tcp_in_use,
            udp_in_use,
        },
    )(input)
}

#[test]
fn test_proc_net_sockstat6() {
    let input = "TCP6: inuse 3
UDP6: inuse 1
UDPLITE6: inuse 0
RAW6: inuse 1
FRAG6: inuse 0 memory 0
";
    let result = proc_net_sockstat6(input).unwrap().1;
    assert_eq!(result.tcp_in_use, 3);
    assert_eq!(result.udp_in_use, 1);
}

/// Stat a mountpoint to gather filesystem statistics
fn stat_mount(mount: ProcMountsData) -> io::Result<Filesystem> {
    let mut info: statvfs = unsafe { mem::zeroed() };
    let target = format!("{}\0", mount.target);
    let result = unsafe { statvfs(target.as_ptr() as *const c_char, &mut info) };
    match result {
        0 => Ok(Filesystem {
            files: (info.f_files as usize).saturating_sub(info.f_ffree as usize),
            files_total: info.f_files as usize,
            files_avail: info.f_favail as usize,
            free: ByteSize::b(info.f_bfree as u64 * info.f_bsize as u64),
            avail: ByteSize::b(info.f_bavail as u64 * info.f_bsize as u64),
            total: ByteSize::b(info.f_blocks as u64 * info.f_bsize as u64),
            name_max: info.f_namemax as usize,
            fs_type: mount.fstype,
            fs_mounted_from: mount.source,
            fs_mounted_on: mount.target,
        }),
        _ => Err(io::Error::last_os_error()),
    }
}

// Parse a line of `/proc/diskstats`
fn proc_diskstats_line(input: &str) -> IResult<&str, BlockDeviceStats> {
    map(
        ws(tuple((
            usize_s, usize_s, word_s, usize_s, usize_s, usize_s, usize_s, usize_s, usize_s,
            usize_s, usize_s, usize_s, usize_s, usize_s,
        ))),
        |(
            _major_number,
            _minor_number,
            name,
            read_ios,
            read_merges,
            read_sectors,
            read_ticks,
            write_ios,
            write_merges,
            write_sectors,
            write_ticks,
            in_flight,
            io_ticks,
            time_in_queue,
        )| BlockDeviceStats {
            name: name.to_string(),
            read_ios,
            read_merges,
            read_sectors,
            read_ticks,
            write_ios,
            write_merges,
            write_sectors,
            write_ticks,
            in_flight,
            io_ticks,
            time_in_queue,
        },
    )(input)
}

// Parse `/proc/diskstats` to get a Vec<BlockDeviceStats>
fn proc_diskstats(input: &str) -> IResult<&str, Vec<BlockDeviceStats>> {
    many0(ws(map_res(not_line_ending, |input| {
        proc_diskstats_line(input)
            .map(|(_, res)| res)
            .map_err(|_| ())
    })))(input)
}

#[test]
fn test_proc_diskstats() {
    let input = " 259       0 nvme0n1 142537 3139 15957288 470540 1235382 57191 140728002 5369037 0 1801270 5898257 0 0 0 0 102387 58679
 259       1 nvme0n1p1 767 2505 20416 1330 2 0 2 38 0 200 1369 0 0 0 0 0 0
 259       2 nvme0n1p2 65 0 4680 37 0 0 0 0 0 44 37 0 0 0 0 0 0
 259       3 nvme0n1p3 141532 634 15927512 469040 1132993 57191 140728000 5308878 0 1801104 5777919 0 0 0 0 0 0
";
    let result = proc_diskstats(input).unwrap().1;
    assert_eq!(result.len(), 4);
    assert_eq!(&result[3].name, "nvme0n1p3");
    assert_eq!(result[3].read_ios, 141532);
    assert_eq!(result[3].write_ios, 1132993);
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

    fn cpu_time(&self) -> io::Result<Vec<CpuTime>> {
        cpu_time()
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
        read_file("/proc/stat").and_then(|data| {
            data.lines()
                .find(|line| line.starts_with("btime "))
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Could not find btime in /proc/stat",
                ))
                .and_then(|line| {
                    let timestamp_str = line
                        .strip_prefix("btime ")
                        .expect("line starts with 'btime '");
                    timestamp_str
                        .parse::<i64>()
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
                        .and_then(|timestamp| {
                            OffsetDateTime::from_unix_timestamp(timestamp).map_err(|err| {
                                io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                            })
                        })
                })
        })
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
                full += value_from_file::<i32>(&(s.to_string() + "/energy_full"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_full")))?;
                now += value_from_file::<i32>(&(s.to_string() + "/energy_now"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/charge_now")))?;
                current += value_from_file::<i32>(&(s.to_string() + "/power_now"))
                    .or_else(|_| value_from_file::<i32>(&(s.to_string() + "/current_now")))?;
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
        read_file("/proc/mounts")
            .and_then(|data| {
                proc_mounts(&data)
                    .map(|(_, res)| res)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
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
                proc_mounts(&data)
                    .map(|(_, res)| res)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
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
        let stats: Vec<BlockDeviceStats> = read_file("/proc/diskstats").and_then(|data| {
            proc_diskstats(&data)
                .map(|(_, res)| res)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
        })?;

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
        read_file("/sys/class/thermal/thermal_zone0/temp")
            .or(read_file("/sys/class/hwmon/hwmon0/temp1_input"))
            .and_then(|data| match data.trim().parse::<f32>() {
                Ok(x) => Ok(x),
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Could not parse float",
                )),
            })
            .map(|num| num / 1000.0)
    }

    fn socket_stats(&self) -> io::Result<SocketStats> {
        let sockstats: ProcNetSockStat = read_file("/proc/net/sockstat").and_then(|data| {
            proc_net_sockstat(&data)
                .map(|(_, res)| res)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
        })?;
        let sockstats6: ProcNetSockStat6 = read_file("/proc/net/sockstat6").and_then(|data| {
            proc_net_sockstat6(&data)
                .map(|(_, res)| res)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
        })?;
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

impl PlatformMemory {
    // Retrieve platform memory information
    fn new() -> io::Result<Self> {
        memory_stats()
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
