extern crate systemstat;

use std::thread;
use std::time::Duration;
use systemstat::{System, Platform};

fn main() {
    let sys = System::new();

    let mounts = sys.mounts().unwrap();
    println!("\nMounts:");
    for mount in mounts.iter() {
        println!("{} ---{}---> {} (available {} of {} bytes)",
                 mount.fs_mounted_from, mount.fs_type, mount.fs_mounted_on, mount.avail_bytes, mount.total_bytes);
    }

    let netifs = sys.networks().unwrap();
    println!("\nNetworks:");
    for netif in netifs.values() {
        println!("{} ({:?})", netif.name, netif.addrs);
    }

    let mem = sys.memory().unwrap();
    println!("\nMemory: {} KiB active, {} KiB inact, {} KiB wired, {} KiB cache, {} KiB free",
             mem.active_kb, mem.inactive_kb, mem.wired_kb, mem.cache_kb, mem.free_kb);

    let loadavg = sys.load_average().unwrap();
    println!("\nLoad average: {} {} {}", loadavg.one, loadavg.five, loadavg.fifteen);

    let cpu = sys.cpu_load_aggregate().unwrap();
    println!("\nMeasuring CPU load...");
    thread::sleep(Duration::from_secs(1));
    let cpu = cpu.done().unwrap();
    println!("CPU load: {}% user, {}% nice, {}% system, {}% intr, {}% idle ",
             cpu.user * 100.0, cpu.nice * 100.0, cpu.system * 100.0, cpu.interrupt * 100.0, cpu.idle * 100.0);
}
