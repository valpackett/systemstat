[![crates.io](https://img.shields.io/crates/v/systemstat.svg)](https://crates.io/crates/systemstat)
[![API Docs](https://docs.rs/systemstat/badge.svg)](https://docs.rs/systemstat/)
[![unlicense](https://img.shields.io/badge/un-license-green.svg?style=flat)](https://unlicense.org)

# systemstat

A Rust library for getting system information/statistics:

- CPU load
- load average
- memory usage
- uptime / boot time
- battery life
- filesystem mounts (and disk usage)
- disk I/O statistics
- network interfaces
- network traffic statistics
- CPU temperature

Unlike [sys-info-rs](https://github.com/FillZpp/sys-info-rs), this one is written purely in Rust.

Supported platforms (roughly ordered by completeness of support):

- FreeBSD
- Linux
- OpenBSD
- Windows
- macOS
- NetBSD
- *more coming soon*

## Usage

See [examples/info.rs](https://github.com/unrelentingtech/systemstat/blob/master/examples/info.rs).

## Contributing

Please feel free to submit pull requests!

By participating in this project you agree to follow the [Contributor Code of Conduct](https://www.contributor-covenant.org/version/1/4/code-of-conduct/) and to release your contributions under the Unlicense.

## License

This is free and unencumbered software released into the public domain.  
For more information, please refer to the `UNLICENSE` file or [unlicense.org](https://unlicense.org).
