use std::{io, path};
use data::*;

pub trait Platform {
    fn new() -> Self;
    fn load_average(&self) -> io::Result<LoadAverage>;
    fn mounts(&self) -> io::Result<Vec<Filesystem>>;
    fn mount_at<P: AsRef<path::Path>>(&self, path: P) -> io::Result<Filesystem>;
}
