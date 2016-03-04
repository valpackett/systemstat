extern crate systemstat;

use std::thread;
use std::time::Duration;
use systemstat::{System, Platform};

fn main() {
    let sys = System::new();

    let mounts = sys.mounts().unwrap();
    println!("\nMounts:");
    for mount in mounts.iter() {
        println!("{} ---{}---> {} (available {} of {})",
                 mount.fs_mounted_from, mount.fs_type, mount.fs_mounted_on, mount.avail, mount.total);
    }

    let netifs = sys.networks().unwrap();
    println!("\nNetworks:");
    for netif in netifs.values() {
        println!("{} ({:?})", netif.name, netif.addrs);
    }

    if let Ok(battery) = sys.battery_life() {
        print!("\nBattery: {}%, {}h{}m remaining",
               battery.remaining_capacity*100.0,
               battery.remaining_time.as_secs() / 60,
               battery.remaining_time.as_secs() % 60);
    } else {
        print!("\nNo battery detected");
    }
    println!(", AC power: {}", sys.on_ac_power().unwrap());

    let mem = sys.memory().unwrap();
    println!("\nMemory: {}/{} ({:?})", mem.free, mem.total, mem.platform_memory);

    let loadavg = sys.load_average().unwrap();
    println!("\nLoad average: {} {} {}", loadavg.one, loadavg.five, loadavg.fifteen);

    let cpu = sys.cpu_load_aggregate().unwrap();
    println!("\nMeasuring CPU load...");
    thread::sleep(Duration::from_secs(1));
    let cpu = cpu.done().unwrap();
    println!("CPU load: {}% user, {}% nice, {}% system, {}% intr, {}% idle ",
             cpu.user * 100.0, cpu.nice * 100.0, cpu.system * 100.0, cpu.interrupt * 100.0, cpu.idle * 100.0);
}
