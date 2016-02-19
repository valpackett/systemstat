pub mod common;
pub use self::common::*;

#[cfg(target_os = "freebsd")]
pub mod freebsd;
#[cfg(target_os = "freebsd")]
pub use self::freebsd::PlatformImpl;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_average() {
        let load = PlatformImpl.load_average().unwrap();
        assert!(load.one > 0.00001 && load.five > 0.00001 && load.fifteen > 0.00001);
    }

    #[test]
    fn test_mounts() {
        let mounts = PlatformImpl.mounts().unwrap();
        assert!(mounts.len() > 0);
        assert!(mounts.iter().find(|m| m.fs_mounted_on == "/").unwrap().fs_mounted_on == "/");
    }

    #[test]
    fn test_mount_at() {
        let mount = PlatformImpl.mount_at("/").unwrap();
        assert!(mount.fs_mounted_on == "/");
    }

}
