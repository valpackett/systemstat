use data::*;
use libc::{c_int, freeifaddrs, getifaddrs, ifaddrs, sockaddr, sockaddr_in6, AF_INET, AF_INET6};
use std::{ffi, io, mem, ptr};

pub fn load_average() -> io::Result<LoadAverage> {
    let mut loads: [f64; 3] = [0.0, 0.0, 0.0];
    if unsafe { getloadavg(&mut loads[0], 3) } != 3 {
        return Err(io::Error::new(io::ErrorKind::Other, "getloadavg() failed"));
    }
    Ok(LoadAverage {
        one: loads[0] as f32,
        five: loads[1] as f32,
        fifteen: loads[2] as f32,
    })
}

pub fn networks() -> io::Result<BTreeMap<String, Network>> {
    let mut ifap: *mut ifaddrs = ptr::null_mut();
    if unsafe { getifaddrs(&mut ifap) } != 0 {
        return Err(io::Error::new(io::ErrorKind::Other, "getifaddrs() failed"));
    }
    let ifirst = ifap;
    let mut result = BTreeMap::new();
    while ifap != ptr::null_mut() {
        let ifa = unsafe { (*ifap) };
        let name = unsafe {
            ffi::CStr::from_ptr(ifa.ifa_name)
                .to_string_lossy()
                .into_owned()
        };
        let mut entry = result.entry(name.clone()).or_insert(Network {
            name: name,
            addrs: Vec::new(),
        });
        let addr = parse_addr(ifa.ifa_addr);
        if addr != IpAddr::Unsupported {
            entry.addrs.push(NetworkAddrs {
                addr: addr,
                netmask: parse_addr(ifa.ifa_netmask),
            });
        }
        ifap = unsafe { (*ifap).ifa_next };
    }
    unsafe { freeifaddrs(ifirst) };
    Ok(result)
}

fn parse_addr(aptr: *const sockaddr) -> IpAddr {
    if aptr == ptr::null() {
        return IpAddr::Empty;
    }
    let addr = unsafe { *aptr };
    match addr.sa_family as i32 {
        AF_INET => IpAddr::V4(Ipv4Addr::new(
            addr.sa_data[2] as u8,
            addr.sa_data[3] as u8,
            addr.sa_data[4] as u8,
            addr.sa_data[5] as u8,
        )),
        AF_INET6 => {
            // This is horrible.
            let addr6: *const sockaddr_in6 = unsafe { mem::transmute(aptr) };
            let mut a: [u8; 16] = unsafe { (*addr6).sin6_addr.s6_addr };
            &mut a[..].reverse();
            let a: [u16; 8] = unsafe { mem::transmute(a) };
            IpAddr::V6(Ipv6Addr::new(
                a[7], a[6], a[5], a[4], a[3], a[2], a[1], a[0],
            ))
        }
        _ => IpAddr::Unsupported,
    }
}

#[link(name = "c")]
extern "C" {
    fn getloadavg(loadavg: *mut f64, nelem: c_int) -> c_int;
}
