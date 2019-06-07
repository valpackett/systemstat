//! This module reexports the OS-specific module that actually implements Platform.
pub mod common;
pub use self::common::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::PlatformImpl;

#[cfg(unix)]
pub mod unix;

#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
pub mod bsd;

#[cfg(target_os = "freebsd")]
pub mod freebsd;
#[cfg(target_os = "freebsd")]
pub use self::freebsd::PlatformImpl;

#[cfg(target_os = "openbsd")]
pub mod openbsd;
#[cfg(target_os = "openbsd")]
pub use self::openbsd::PlatformImpl;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::PlatformImpl;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod linux;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use self::linux::PlatformImpl;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use std::path::PathBuf;

    #[test]
    fn test_cpu_load() {
        let load = PlatformImpl::new().cpu_load().unwrap();
        thread::sleep(Duration::from_millis(300));
        let load = load.done().unwrap();
        assert!(load.len() >= 1);
        for cpu in load.iter() {
            let sum = cpu.user + cpu.nice + cpu.system + cpu.interrupt + cpu.idle;
            assert!(sum > 0.95 && sum < 1.05);
        }
    }

    #[test]
    fn test_cpu_load_aggregate() {
        let cpu = PlatformImpl::new().cpu_load_aggregate().unwrap();
        thread::sleep(Duration::from_millis(300));
        let cpu = cpu.done().unwrap();
        let sum = cpu.user + cpu.nice + cpu.system + cpu.interrupt + cpu.idle;
        assert!(sum > 0.95 && sum < 1.05);
    }

    #[test]
    fn test_load_average() {
        let load = PlatformImpl::new().load_average().unwrap();
        assert!(load.one > 0.00001 && load.five > 0.00001 && load.fifteen > 0.00001);
    }

    #[test]
    fn test_memory() {
        let mem = PlatformImpl::new().memory().unwrap();
        assert!(mem.free.as_usize() > 1024 && mem.total.as_usize() > 1024);
    }

    #[test]
    fn test_battery_life() {
        if let Ok(bat) = PlatformImpl::new().battery_life() {
            assert!(bat.remaining_capacity <= 100.0 && bat.remaining_capacity >= 0.0);
        }
    }

    #[test]
    fn test_on_ac_power() {
        PlatformImpl::new().on_ac_power().unwrap();
    }

    #[test]
    fn test_mounts() {
        let mounts = PlatformImpl::new().mounts().unwrap();
        assert!(mounts.len() > 0);
        assert!(mounts.iter().find(|m| m.fs_mounted_on == "/").unwrap().fs_mounted_on == "/");
    }

    #[test]
    fn test_mount_at() {
        let path = PathBuf::from("/");
        let mount = PlatformImpl::new().mount_at(path).unwrap();
        assert!(mount.fs_mounted_on == "/");
    }

    #[test]
    fn test_networks() {
        let networks = PlatformImpl::new().networks().unwrap();
        assert!(networks.values().filter(|n| n.name == "lo" || n.name == "lo0").next().unwrap().addrs.len() > 0);
    }

    #[test]
    fn test_cpu_measurement_is_send() {
        use crate::{DelayedMeasurement, CPULoad};
        fn take_delayed(dm: DelayedMeasurement<Vec<CPULoad>>) {
            use std::thread;
            thread::spawn(move || dm);
        }
    }

}
