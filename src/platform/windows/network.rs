use winapi::shared::ws2def::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR}; 
use winapi::shared::winerror::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS}; 
use winapi::shared::ws2ipdef::SOCKADDR_IN6_LH;
use winapi::shared::minwindef::*;
use winapi::ctypes::*;
use libc::{size_t, c_void,  malloc, free};

use data::*;

use std::{io, ptr};
use std::io::Error;
use std::mem;
use std::collections::BTreeMap;

// unions with non-`Copy` fields are unstable (see issue #32836)
// #[repr(C)]
// union AlimentOrLengthIfIndex {
//     aliment: c_ulonglong,
//     length_ifindex: LengthIfIndex,
// }

#[repr(C)]
struct LengthIfIndex{
    length: ULONG,
    ifindex: DWORD,
}

#[repr(C)]
struct LengthFlags{
    length: ULONG,
    flags: DWORD,
}
 
#[repr(C)]
struct SoketAddress{ 
    lp_sockaddr: *mut SOCKADDR,
    i_sockaddr_length: c_int
}

#[repr(C)]
struct IpAdapterPrefix{
    aol: LengthIfIndex,
    next: *mut IpAdapterPrefix,
    address: SoketAddress,
    prefix_length: ULONG,
}

#[repr(C)]
struct IpAdapterUnicastAddress {
    aol: LengthFlags,
    next: *mut IpAdapterUnicastAddress,
    address: SoketAddress,
    prefix_origin: c_int,
    suffix_origin: c_int,
    dad_state: c_int,
    valid_lifetime: ULONG,
    preferred_lifetime: ULONG,
    lease_lifetime: ULONG,
}

const MAX_ADAPTER_ADDRESS_LENGTH: usize = 8;

#[repr(C)]
struct IpAdapterAddresses{
    aol: LengthIfIndex,
    next: *mut IpAdapterAddresses,
    adapter_name:  *mut c_char,
    first_unicass_address: *mut IpAdapterUnicastAddress, 
    first_anycass_address: *const c_void,
    first_multicass_address: *const c_void,
    first_dns_server_address: *const c_void,
    dns_suffix: *mut wchar_t,
    description: *mut wchar_t,
    friendly_name:  *mut wchar_t,
    physical_address: [u8; MAX_ADAPTER_ADDRESS_LENGTH],
    physical_address_length: DWORD,
    flags: DWORD,
    mtu: DWORD,
    if_type: DWORD,
    oper_status: c_int,
    ipv6_if_index: DWORD,
    zone_indices:  [DWORD;16],
    first_prefix: *mut IpAdapterPrefix,
}

// https://msdn.microsoft.com/en-us/library/aa365915(v=vs.85).aspx
// C:\Program Files (x86)\Windows Kits\8.1\Include\um\IPHlpApi.h
#[link(name = "Iphlpapi")]
extern "system" {
    fn GetAdaptersAddresses(
        family: ULONG,
        flags: ULONG,
        reserved: *const c_void,
        addresses: *mut IpAdapterAddresses,
        size: *mut ULONG,
    ) -> ULONG;
}

const WORKING_BUFFER_SIZEL: size_t = 15000;

pub fn interfaces() -> io::Result<BTreeMap<String, Network>> {
    let mut new_size:ULONG = WORKING_BUFFER_SIZEL as ULONG;
    let mut p_adapter: *mut IpAdapterAddresses;
    loop {
        unsafe {
            p_adapter = malloc( WORKING_BUFFER_SIZEL) as *mut IpAdapterAddresses;
            if p_adapter.is_null() {
                panic!("Failed: malloc!");
            }
            let res_code = GetAdaptersAddresses (
                0, 
                AF_UNSPEC as ULONG, // ipv4 & ipv6
                ptr::null(),
                p_adapter,
                &mut new_size as *mut ULONG
            );
            match res_code {
                // 0
                ERROR_SUCCESS => break,
                // 111, retry
                ERROR_BUFFER_OVERFLOW => {
                    new_size*=2;
                    free(p_adapter as *mut c_void);
                    continue;
                }
                _=> {
                    return Err( Error::last_os_error());
                }
            } 
        }
    }

    let mut map = BTreeMap::new();
    // key->adapter_name, name-> friendly_name, maybe should use the adapter_name all.
    unsafe {
        let mut cur_p_adapter = p_adapter;
        while !cur_p_adapter.is_null() {
            // name, mac, etc
            let adapter_name = c_char_array_to_string( (*cur_p_adapter).adapter_name);
            // println!("adapter_name : {}", adapter_name);

            // let dns_suffix = u16_array_to_string((*cur_p_adapter).dns_suffix);
            // println!("dns_suffix   : {}", dns_suffix);

            let friendly_name =  u16_array_to_string((*cur_p_adapter).friendly_name);
            // println!("friendly_name: {}", friendly_name);

            // let description = u16_array_to_string((*cur_p_adapter).description);
            // println!("description  : {}", description);

            // let mac = physical_address_to_string(&(*cur_p_adapter).physical_address, (*cur_p_adapter).physical_address_length);
            // println!("mac          : {}", mac);
            
            let mut addrs = Vec::new();
            // ip
            let mut cur_p_addr = (*cur_p_adapter).first_unicass_address;
            while !cur_p_addr.is_null() {
                let addr = parse_addr((*cur_p_addr).address.lp_sockaddr);
                addrs.push(addr);
               // println!("{:?}", addr);
               // next addr              
               cur_p_addr =(*cur_p_addr).next;
            }
            let network = Network {
                name: friendly_name,
                addrs: addrs.iter().map(|addr| NetworkAddrs {addr: addr.clone(), netmask: IpAddr::Unsupported}).collect()
            };
            map.insert(adapter_name,network);

            // next adapter
            cur_p_adapter = (*cur_p_adapter).next;
        }
    }

    unsafe { free(p_adapter as *mut c_void); }
    Ok(map)
}

use std::slice::from_raw_parts;
fn u16_array_to_string(p: *const u16) ->String {
    use std::char::{decode_utf16, REPLACEMENT_CHARACTER};    
    unsafe {
        if p.is_null() {
            return String::new();
        }
        let mut amt = 0usize;
        while !p.offset(amt as isize).is_null() &&  *p.offset(amt as isize) != 0u16 {
            amt+=1;
        }
        let u16s = from_raw_parts(p, amt);
        decode_utf16(u16s.iter().cloned())
        .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
        .collect::<String>()
    }
}

use std::ffi::CStr;
fn c_char_array_to_string(p: *const c_char) -> String {
    unsafe {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

use std::io::Write;

fn physical_address_to_string(array: &[u8;8], length: DWORD) -> String {
    let mut bytes = Vec::with_capacity(length as usize);
    for idx in 0..length as usize {
        if idx == 0  {
            write!(&mut bytes, "{:02X}", array[idx]).unwrap();
        } else {
            write!(&mut bytes, "-{:02X}", array[idx]).unwrap();
        }
    }
    String::from_utf8_lossy(&bytes[..]).into_owned()
}


// Thanks , copy from unix.rs and some modify
fn parse_addr(aptr: *const SOCKADDR) -> IpAddr {
    if aptr == ptr::null() {
        return IpAddr::Empty;
    }
    let addr = unsafe { *aptr };
    match addr.sa_family as i32 {
        AF_INET => IpAddr::V4(Ipv4Addr::new(addr.sa_data[2] as u8, addr.sa_data[3] as u8,
                                            addr.sa_data[4] as u8, addr.sa_data[5] as u8)),
        AF_INET6 => {
            // This is horrible.
            let addr6: *const SOCKADDR_IN6_LH = unsafe { mem::transmute(aptr) };
            let mut a: [u8; 16] = unsafe { *(*addr6).sin6_addr.u.Byte() };
            &mut a[..].reverse();
            let a: [u16; 8] = unsafe { mem::transmute(a) };
            IpAddr::V6(Ipv6Addr::new(a[7], a[6], a[5], a[4], a[3], a[2], a[1], a[0]))
        },
        _ => IpAddr::Unsupported,
    }
}