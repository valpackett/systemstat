#[derive(Debug, Clone)]
pub struct LoadAverage {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

#[derive(Debug, Clone)]
pub struct Filesystem {
    pub files: u64,
    pub free_bytes: u64,
    pub avail_bytes: u64,
    pub total_bytes: u64,
    pub name_max: u64,
    pub fs_type: String,
    pub fs_mounted_from: String,
    pub fs_mounted_on: String
}
