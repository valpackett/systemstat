use winapi::ctypes::c_ulong;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::shared::ws2def::{AF_INET, AF_INET6};

use super::last_os_error;
use data::*;

use std::io;

#[derive(Debug, Default)]
#[repr(C)]
struct TcpStats {
    rto_algorithm: c_ulong,
    rto_min: c_ulong,
    rto_max: c_ulong,
    max_conn: c_ulong,
    active_opens: c_ulong,
    passive_opens: c_ulong,
    attemp_fails: c_ulong,
    estab_resets: c_ulong,
    curr_estab: c_ulong,
    in_segs: c_ulong,
    out_segs: c_ulong,
    retrans_segs: c_ulong,
    in_errs: c_ulong,
    out_rsts: c_ulong,
    num_conns: c_ulong,
}

#[derive(Debug, Default)]
#[repr(C)]
struct UdpStats {
    in_datagrams: c_ulong,
    no_ports: c_ulong,
    in_errors: c_ulong,
    out_datagrams: c_ulong,
    num_addrs: c_ulong,
}

#[link(name = "Iphlpapi")]
extern "system" {
    // https://msdn.microsoft.com/en-us/library/aa366023(v=vs.85).aspx
    fn GetTcpStatisticsEx(pStats: *mut TcpStats, dwFamily: c_ulong) -> c_ulong;
    // https://msdn.microsoft.com/en-us/library/aa366031(v=vs.85).aspx
    fn GetUdpStatisticsEx(pStats: *mut UdpStats, dwFamily: c_ulong) -> c_ulong;
}

pub fn get() -> io::Result<SocketStats> {
    let mut tcp4 = TcpStats::default();
    let mut tcp6 = TcpStats::default();
    let mut udp4 = UdpStats::default();
    let mut udp6 = UdpStats::default();

    if ERROR_SUCCESS != unsafe { GetTcpStatisticsEx(&mut tcp4 as *mut _, AF_INET as c_ulong) } {
        last_os_error()?;
    }
    if ERROR_SUCCESS != unsafe { GetTcpStatisticsEx(&mut tcp6 as *mut _, AF_INET6 as c_ulong) } {
        last_os_error()?;
    }

    if ERROR_SUCCESS != unsafe { GetUdpStatisticsEx(&mut udp4 as *mut _, AF_INET as c_ulong) } {
        last_os_error()?;
    }
    if ERROR_SUCCESS != unsafe { GetUdpStatisticsEx(&mut udp6 as *mut _, AF_INET6 as c_ulong) } {
        last_os_error()?;
    }

    // println!("4: {:?}", tcp4 );
    // println!("6: {:?}", tcp6 );
    // println!("4: {:?}", udp4 );
    // println!("6: {:?}", udp6 );

    let stat = SocketStats {
        tcp_sockets_in_use: tcp4.num_conns as usize,
        tcp_sockets_orphaned: 0, // ? who or how to compute?
        tcp6_sockets_in_use: tcp6.num_conns as usize,
        udp_sockets_in_use: udp4.num_addrs as usize,
        udp6_sockets_in_use: udp6.num_addrs as usize,
    };

    Ok(stat)
}
