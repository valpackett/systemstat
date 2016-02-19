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

}
