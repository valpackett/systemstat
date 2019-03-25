use libc::{c_void, free, malloc, size_t, uint8_t};
use winapi::ctypes::*;
use winapi::shared::minwindef::*;
use winapi::shared::winerror::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
use winapi::shared::ws2def::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR};
use winapi::shared::ws2ipdef::SOCKADDR_IN6_LH;

use super::{c_char_array_to_string, last_os_error, u16_array_to_string};
use data::*;

use std::collections::BTreeMap;
use std::io::Write;
use std::{io, mem, ptr};

// unions with non-`Copy` fields are unstable (see issue #32836)
// #[repr(C)]
// union AlimentOrLengthIfIndex {
//     aliment: c_ulonglong,
//     length_ifindex: LengthIfIndex,
// }

#[repr(C)]
struct LengthIfIndex {
    length: ULONG,
    ifindex: DWORD,
}

#[repr(C)]
struct LengthFlags {
    length: ULONG,
    flags: DWORD,
}

#[repr(C)]
struct SoketAddress {
    lp_sockaddr: *mut SOCKADDR,
    i_sockaddr_length: c_int,
}

#[repr(C)]
struct IpAdapterPrefix {
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
    on_link_prefix_length: uint8_t,
}

const MAX_ADAPTER_ADDRESS_LENGTH: usize = 8;

#[repr(C)]
struct IpAdapterAddresses {
    aol: LengthIfIndex,
    next: *mut IpAdapterAddresses,
    adapter_name: *mut c_char,
    first_unicass_address: *mut IpAdapterUnicastAddress,
    first_anycass_address: *const c_void,
    first_multicass_address: *const c_void,
    first_dns_server_address: *const c_void,
    dns_suffix: *mut wchar_t,
    description: *mut wchar_t,
    friendly_name: *mut wchar_t,
    physical_address: [u8; MAX_ADAPTER_ADDRESS_LENGTH],
    physical_address_length: DWORD,
    flags: DWORD,
    mtu: DWORD,
    if_type: DWORD,
    oper_status: c_int,
    ipv6_if_index: DWORD,
    zone_indices: [DWORD; 16],
    first_prefix: *mut IpAdapterPrefix,
}

// https://msdn.microsoft.com/en-us/library/aa365915(v=vs.85).aspx
// https://msdn.microsoft.com/zh-cn/library/windows/desktop/aa366066(d=printer,v=vs.85).aspx
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

pub fn get() -> io::Result<BTreeMap<String, Network>> {
    let mut new_size: ULONG = WORKING_BUFFER_SIZEL as ULONG;
    let mut p_adapter: *mut IpAdapterAddresses;
    loop {
        unsafe {
            p_adapter = malloc(WORKING_BUFFER_SIZEL) as *mut IpAdapterAddresses;
            if p_adapter.is_null() {
                panic!("Failed: malloc!");
            }
            let res_code = GetAdaptersAddresses(
                0,
                AF_UNSPEC as ULONG, // ipv4 & ipv6
                ptr::null(),
                p_adapter,
                &mut new_size as *mut ULONG,
            );
            match res_code {
                // 0
                ERROR_SUCCESS => break,
                // 111, retry
                ERROR_BUFFER_OVERFLOW => {
                    new_size *= 2;
                    free(p_adapter as *mut c_void);
                    continue;
                }
                _ => {
                    last_os_error()?;
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
            let adapter_name = c_char_array_to_string((*cur_p_adapter).adapter_name);
            // println!("adapter_name : {}", adapter_name);

            // let dns_suffix = u16_array_to_string((*cur_p_adapter).dns_suffix);
            // println!("dns_suffix   : {}", dns_suffix);

            let friendly_name = u16_array_to_string((*cur_p_adapter).friendly_name);
            // println!("friendly_name: {}", friendly_name);

            // let description = u16_array_to_string((*cur_p_adapter).description);
            // println!("description  : {}", description);

            // let mac = physical_address_to_string(&(*cur_p_adapter).physical_address, (*cur_p_adapter).physical_address_length);
            // println!("mac          : {}", mac);

            let mut addrs = Vec::new();
            // ip
            let mut cur_p_addr = (*cur_p_adapter).first_unicass_address;
            while !cur_p_addr.is_null() {
                let addr = parse_addr_and_netmask(
                    (*cur_p_addr).address.lp_sockaddr,
                    (*cur_p_addr).on_link_prefix_length,
                );
                addrs.push(addr);
                // println!("{:?}", addr);
                // next addr
                cur_p_addr = (*cur_p_addr).next;
            }
            let network = Network {
                name: friendly_name,
                addrs: addrs,
            };
            map.insert(adapter_name, network);

            // next adapter
            cur_p_adapter = (*cur_p_adapter).next;
        }
    }

    unsafe {
        free(p_adapter as *mut c_void);
    }
    Ok(map)
}

fn physical_address_to_string(array: &[u8; 8], length: DWORD) -> String {
    let mut bytes = Vec::with_capacity(length as usize);
    for idx in 0..length as usize {
        if idx == 0 {
            write!(&mut bytes, "{:02X}", array[idx]).unwrap();
        } else {
            write!(&mut bytes, "-{:02X}", array[idx]).unwrap();
        }
    }
    String::from_utf8_lossy(&bytes[..]).into_owned()
}

// Thanks , copy from unix.rs and some modify
fn parse_addr_and_netmask(aptr: *const SOCKADDR, net_bits: uint8_t) -> NetworkAddrs {
    if aptr == ptr::null() {
        return NetworkAddrs {
            addr: IpAddr::Empty,
            netmask: IpAddr::Empty,
        };
    }
    let addr = unsafe { *aptr };
    match addr.sa_family as i32 {
        AF_INET => {
            let addr = IpAddr::V4(Ipv4Addr::new(
                addr.sa_data[2] as u8,
                addr.sa_data[3] as u8,
                addr.sa_data[4] as u8,
                addr.sa_data[5] as u8,
            ));
            let netmask = if net_bits <= 32 {
                IpAddr::V4(netmask_v4(net_bits))
            } else {
                IpAddr::Empty
            };
            NetworkAddrs { addr, netmask }
        }
        AF_INET6 => {
            // This is horrible.
            let addr6: *const SOCKADDR_IN6_LH = unsafe { mem::transmute(aptr) };
            let mut a: [u8; 16] = unsafe { *(*addr6).sin6_addr.u.Byte() };
            &mut a[..].reverse();
            let a: [u16; 8] = unsafe { mem::transmute(a) };
            let addr = IpAddr::V6(Ipv6Addr::new(
                a[7], a[6], a[5], a[4], a[3], a[2], a[1], a[0],
            ));
            let netmask = if net_bits <= 128 {
                IpAddr::V6(netmask_v6(net_bits))
            } else {
                IpAddr::Empty
            };
            NetworkAddrs { addr, netmask }
        }
        _ => NetworkAddrs {
            addr: IpAddr::Empty,
            netmask: IpAddr::Empty,
        },
    }
}

// This faster than [u8;4], but v6 is slower if use this..
// And the scan() method is slower also.
fn netmask_v4(bits: u8) -> Ipv4Addr {
    let mut i = (0..4).map(|idx| {
        let idx8 = idx << 3;
        match (bits as usize > idx8, bits as usize > idx8 + 8) {
            (true, true) => 255,
            (true, false) => 255u8.wrapping_shl((8 - bits % 8) as u32),
            _ => 0,
        }
    });
    Ipv4Addr::new(
        i.next().unwrap(),
        i.next().unwrap(),
        i.next().unwrap(),
        i.next().unwrap(),
    )
}

fn netmask_v6(bits: u8) -> Ipv6Addr {
    let mut tmp = [0u16; 8];
    (0..8).for_each(|idx| {
        let idx16 = idx << 4;
        match (bits as usize > idx16, bits as usize > idx16 + 16) {
            (true, true) => {
                tmp[idx] = 0xffff;
            }
            (true, false) => {
                tmp[idx] = 0xffffu16.wrapping_shl((16 - bits % 16) as u32);
            }
            _ => {}
        }
    });
    Ipv6Addr::new(
        tmp[0], tmp[1], tmp[2], tmp[3], tmp[4], tmp[5], tmp[6], tmp[7],
    )
}

#[test]
fn netmask_v4_test() {
    vec![
        (0, "0.0.0.0"),
        (1, "128.0.0.0"),
        (2, "192.0.0.0"),
        (3, "224.0.0.0"),
        (4, "240.0.0.0"),
        (5, "248.0.0.0"),
        (6, "252.0.0.0"),
        (7, "254.0.0.0"),
        (8, "255.0.0.0"),
        (9, "255.128.0.0"),
        (10, "255.192.0.0"),
        (11, "255.224.0.0"),
        (12, "255.240.0.0"),
        (13, "255.248.0.0"),
        (14, "255.252.0.0"),
        (15, "255.254.0.0"),
        (16, "255.255.0.0"),
        (17, "255.255.128.0"),
        (18, "255.255.192.0"),
        (19, "255.255.224.0"),
        (20, "255.255.240.0"),
        (21, "255.255.248.0"),
        (22, "255.255.252.0"),
        (23, "255.255.254.0"),
        (24, "255.255.255.0"),
        (25, "255.255.255.128"),
        (26, "255.255.255.192"),
        (27, "255.255.255.224"),
        (28, "255.255.255.240"),
        (29, "255.255.255.248"),
        (30, "255.255.255.252"),
        (31, "255.255.255.254"),
        (32, "255.255.255.255"),
    ]
    .into_iter()
    .for_each(|(i, addr)| assert_eq!(netmask_v4(i), addr.parse::<Ipv4Addr>().unwrap()))
}

#[test]
fn netmask_v6_test() {
    vec![
        (0, "::"),
        (1, "8000::"),
        (2, "c000::"),
        (3, "e000::"),
        (4, "f000::"),
        (5, "f800::"),
        (6, "fc00::"),
        (7, "fe00::"),
        (8, "ff00::"),
        (9, "ff80::"),
        (10, "ffc0::"),
        (11, "ffe0::"),
        (12, "fff0::"),
        (13, "fff8::"),
        (14, "fffc::"),
        (15, "fffe::"),
        (16, "ffff::"),
        (17, "ffff:8000::"),
        (18, "ffff:c000::"),
        (19, "ffff:e000::"),
        (20, "ffff:f000::"),
        (21, "ffff:f800::"),
        (22, "ffff:fc00::"),
        (23, "ffff:fe00::"),
        (24, "ffff:ff00::"),
        (25, "ffff:ff80::"),
        (26, "ffff:ffc0::"),
        (27, "ffff:ffe0::"),
        (28, "ffff:fff0::"),
        (29, "ffff:fff8::"),
        (30, "ffff:fffc::"),
        (31, "ffff:fffe::"),
        (32, "ffff:ffff::"),
        (33, "ffff:ffff:8000::"),
        (34, "ffff:ffff:c000::"),
        (35, "ffff:ffff:e000::"),
        (36, "ffff:ffff:f000::"),
        (37, "ffff:ffff:f800::"),
        (38, "ffff:ffff:fc00::"),
        (39, "ffff:ffff:fe00::"),
        (40, "ffff:ffff:ff00::"),
        (41, "ffff:ffff:ff80::"),
        (42, "ffff:ffff:ffc0::"),
        (43, "ffff:ffff:ffe0::"),
        (44, "ffff:ffff:fff0::"),
        (45, "ffff:ffff:fff8::"),
        (46, "ffff:ffff:fffc::"),
        (47, "ffff:ffff:fffe::"),
        (48, "ffff:ffff:ffff::"),
        (49, "ffff:ffff:ffff:8000::"),
        (50, "ffff:ffff:ffff:c000::"),
        (51, "ffff:ffff:ffff:e000::"),
        (52, "ffff:ffff:ffff:f000::"),
        (53, "ffff:ffff:ffff:f800::"),
        (54, "ffff:ffff:ffff:fc00::"),
        (55, "ffff:ffff:ffff:fe00::"),
        (56, "ffff:ffff:ffff:ff00::"),
        (57, "ffff:ffff:ffff:ff80::"),
        (58, "ffff:ffff:ffff:ffc0::"),
        (59, "ffff:ffff:ffff:ffe0::"),
        (60, "ffff:ffff:ffff:fff0::"),
        (61, "ffff:ffff:ffff:fff8::"),
        (62, "ffff:ffff:ffff:fffc::"),
        (63, "ffff:ffff:ffff:fffe::"),
        (64, "ffff:ffff:ffff:ffff::"),
        (65, "ffff:ffff:ffff:ffff:8000::"),
        (66, "ffff:ffff:ffff:ffff:c000::"),
        (67, "ffff:ffff:ffff:ffff:e000::"),
        (68, "ffff:ffff:ffff:ffff:f000::"),
        (69, "ffff:ffff:ffff:ffff:f800::"),
        (70, "ffff:ffff:ffff:ffff:fc00::"),
        (71, "ffff:ffff:ffff:ffff:fe00::"),
        (72, "ffff:ffff:ffff:ffff:ff00::"),
        (73, "ffff:ffff:ffff:ffff:ff80::"),
        (74, "ffff:ffff:ffff:ffff:ffc0::"),
        (75, "ffff:ffff:ffff:ffff:ffe0::"),
        (76, "ffff:ffff:ffff:ffff:fff0::"),
        (77, "ffff:ffff:ffff:ffff:fff8::"),
        (78, "ffff:ffff:ffff:ffff:fffc::"),
        (79, "ffff:ffff:ffff:ffff:fffe::"),
        (80, "ffff:ffff:ffff:ffff:ffff::"),
        (81, "ffff:ffff:ffff:ffff:ffff:8000::"),
        (82, "ffff:ffff:ffff:ffff:ffff:c000::"),
        (83, "ffff:ffff:ffff:ffff:ffff:e000::"),
        (84, "ffff:ffff:ffff:ffff:ffff:f000::"),
        (85, "ffff:ffff:ffff:ffff:ffff:f800::"),
        (86, "ffff:ffff:ffff:ffff:ffff:fc00::"),
        (87, "ffff:ffff:ffff:ffff:ffff:fe00::"),
        (88, "ffff:ffff:ffff:ffff:ffff:ff00::"),
        (89, "ffff:ffff:ffff:ffff:ffff:ff80::"),
        (90, "ffff:ffff:ffff:ffff:ffff:ffc0::"),
        (91, "ffff:ffff:ffff:ffff:ffff:ffe0::"),
        (92, "ffff:ffff:ffff:ffff:ffff:fff0::"),
        (93, "ffff:ffff:ffff:ffff:ffff:fff8::"),
        (94, "ffff:ffff:ffff:ffff:ffff:fffc::"),
        (95, "ffff:ffff:ffff:ffff:ffff:fffe::"),
        (96, "ffff:ffff:ffff:ffff:ffff:ffff::"),
        (97, "ffff:ffff:ffff:ffff:ffff:ffff:8000:0"),
        (98, "ffff:ffff:ffff:ffff:ffff:ffff:c000:0"),
        (99, "ffff:ffff:ffff:ffff:ffff:ffff:e000:0"),
        (100, "ffff:ffff:ffff:ffff:ffff:ffff:f000:0"),
        (101, "ffff:ffff:ffff:ffff:ffff:ffff:f800:0"),
        (102, "ffff:ffff:ffff:ffff:ffff:ffff:fc00:0"),
        (103, "ffff:ffff:ffff:ffff:ffff:ffff:fe00:0"),
        (104, "ffff:ffff:ffff:ffff:ffff:ffff:ff00:0"),
        (105, "ffff:ffff:ffff:ffff:ffff:ffff:ff80:0"),
        (106, "ffff:ffff:ffff:ffff:ffff:ffff:ffc0:0"),
        (107, "ffff:ffff:ffff:ffff:ffff:ffff:ffe0:0"),
        (108, "ffff:ffff:ffff:ffff:ffff:ffff:fff0:0"),
        (109, "ffff:ffff:ffff:ffff:ffff:ffff:fff8:0"),
        (110, "ffff:ffff:ffff:ffff:ffff:ffff:fffc:0"),
        (111, "ffff:ffff:ffff:ffff:ffff:ffff:fffe:0"),
        (112, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:0"),
        (113, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:8000"),
        (114, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:c000"),
        (115, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:e000"),
        (116, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:f000"),
        (117, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:f800"),
        (118, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fc00"),
        (119, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fe00"),
        (120, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ff00"),
        (121, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ff80"),
        (122, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffc0"),
        (123, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffe0"),
        (124, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fff0"),
        (125, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fff8"),
        (126, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fffc"),
        (127, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fffe"),
        (128, "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
    ]
    .into_iter()
    .for_each(|(i, addr)| assert_eq!(netmask_v6(i), addr.parse::<Ipv6Addr>().unwrap()))
}
