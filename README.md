[![crates.io](https://img.shields.io/crates/v/systemstat.svg)](https://crates.io/crates/systemstat)
[![API Docs](https://docs.rs/systemstat/badge.svg)](https://docs.rs/systemstat/)
[![unlicense](https://img.shields.io/badge/un-license-green.svg?style=flat)](http://unlicense.org)

# systemstat

A Rust library for getting system information/statistics:

- CPU load
- load average
- memory usage
- uptime / boot time
- battery life
- filesystem mounts (and disk usage)
- network interfaces

Unlike [sys-info-rs](https://github.com/FillZpp/sys-info-rs), this one is written purely in Rust.

Supported platforms:

- FreeBSD
- OpenBSD (incomplete)
- Linux (incomplete)
- Windows (incomplete)
- *more coming soon*

Originally written for [unixbar](https://github.com/myfreeweb/unixbar) :-)

## Usage

See [examples/info.rs](https://github.com/myfreeweb/systemstat/blob/master/examples/info.rs).

## Contributing

Please feel free to submit pull requests!

By participating in this project you agree to follow the [Contributor Code of Conduct](http://contributor-covenant.org/version/1/4/).

[The list of contributors is available on GitHub](https://github.com/myfreeweb/systemstat/graphs/contributors).

## License

This is free and unencumbered software released into the public domain.  
For more information, please refer to the `UNLICENSE` file or [unlicense.org](http://unlicense.org).
