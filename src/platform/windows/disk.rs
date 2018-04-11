use winapi::ctypes::c_ulong;
use winapi::shared::minwindef::FALSE;
use winapi::um::fileapi::{GetDiskFreeSpaceExW, GetLogicalDriveStringsW, GetVolumeInformationW};
use winapi::um::winnt::ULARGE_INTEGER;

use super::network::u16_array_to_string;
use data::*;

use std::char::{decode_utf16, REPLACEMENT_CHARACTER};
use std::{io, mem, ptr};

pub fn drives() -> io::Result<Vec<Filesystem>> {
    let logical_drives = unsafe { GetLogicalDriveStringsW(0, ptr::null_mut()) };

    let mut u16s = Vec::with_capacity(logical_drives as usize);
    let p_u16s = u16s.as_mut_ptr();

    let get_logical_drives = unsafe { GetLogicalDriveStringsW(logical_drives, p_u16s) };

    // (X://\0)*\0
    if get_logical_drives + 1 != logical_drives {
        Err(io::Error::last_os_error())?;
    }

    unsafe { u16s.set_len(logical_drives as usize) };

    // (X://\0)*\0
    let drives = u16s.split(|c| *c == 0).filter(|iter| !iter.is_empty());

    let mut vec: Vec<Filesystem> = Vec::new();

    for us in drives {
        let name = decode_utf16(us.iter().cloned())
            .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
            .collect::<String>();

        let (max, fs, tag) = get_volume_information(us)?;
        let (total, avail, free) = get_disk_space_ext(us)?;

        let tmp = Filesystem {
            name_max: max as _,
            fs_type: fs,
            fs_mounted_from: tag,
            fs_mounted_on: name,
            total: ByteSize::b(total as usize),
            avail: ByteSize::b(avail as usize),
            free: ByteSize::b(free as usize),
            files: 0, // don't find..
            files_total: 0,
            files_avail: 0,
        };

        vec.push(tmp);
    }

    Ok(vec)
}

// https://msdn.microsoft.com/en-us/library/windows/desktop/aa364993(v=vs.85).aspx
fn get_volume_information(name: &[u16]) -> io::Result<(c_ulong, String, String)> {
    let p_name = name.as_ptr();

    let mut volume_name = Vec::with_capacity(255);
    let p_volume_name = volume_name.as_mut_ptr();

    let mut fs_name = Vec::with_capacity(255);
    let p_fs_name = fs_name.as_mut_ptr();

    let mut volume_serial = Vec::with_capacity(255);
    let p_volume_serial = volume_serial.as_mut_ptr();

    let mut max_component_length: c_ulong = 0;
    let mut fs_flags: c_ulong = 0;

    if FALSE == unsafe {
        GetVolumeInformationW(
            p_name,
            p_volume_name,
            255,
            p_volume_serial,
            &mut max_component_length as *mut _,
            &mut fs_flags as *mut _,
            p_fs_name,
            255,
        )
    } {
        Err(io::Error::last_os_error())?;
    }

    Ok((
        max_component_length,
        u16_array_to_string(p_fs_name),
        u16_array_to_string(p_volume_name),
    ))
}

fn get_disk_space_ext(name: &[u16]) -> io::Result<(u64, u64, u64)> {
    let p_name = name.as_ptr();

    let mut avail: ULARGE_INTEGER = unsafe { mem::uninitialized() };
    let mut total: ULARGE_INTEGER = unsafe { mem::uninitialized() };
    let mut free: ULARGE_INTEGER = unsafe { mem::uninitialized() };

    if FALSE == unsafe {
        GetDiskFreeSpaceExW(
            p_name,
            &mut avail as *mut _,
            &mut total as *mut _,
            &mut free as *mut _,
        )
    } {
        Err(io::Error::last_os_error())?;
    }

    unsafe { Ok((*total.QuadPart(), *avail.QuadPart(), *free.QuadPart())) }
}
