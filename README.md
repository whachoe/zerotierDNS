# Zerotier DNS

A DNS Server written in Rust to resolve zerotier-device names into their IP. See https://zerotier.com/ for more information.

## Getting Started

These instructions will get you a copy of the project up and running on your local machine for development and testing purposes. See deployment for notes on how to deploy the project on a live system.

### Prerequisites

Make sure to have [Rust](https://www.rust-lang.org/en-US/install.html) and [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) installed.

### Installing

```
$ git clone https://github.com/whachoe/zerotierDNS
$ cargo build --release
$ cargo run -- --help
```

```
$ cargo run -- -h
ZerotierDNS 1.0.0
Whachoe <whachoe@gmail.com>
Dns-server for zerotier networks. Resolves names of devices to their IP

USAGE:
    zerotier-dns [OPTIONS] --network <YOUR-ZEROTIER-NETWORK-ID> --token <YOUR-ZEROTIER-API-TOKEN>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -b, --bind <IP to bind on>                  If left out, the app will bind on all available IP's. It's more secure
                                                to bind the IP of your local zerotier-client.
    -p, --proxy <IP of Proxy>                   IP of the server to proxy requests to in case we did not find a match.
                                                [default: 8.8.8.8]
    -n, --network <YOUR-ZEROTIER-NETWORK-ID>    The Network ID of your zerotier-network
    -t, --token <YOUR-ZEROTIER-API-TOKEN>       See https://my.zerotier.com/ to create one
```

## Deployment

Once built, you can find the binary in `target/release`. Just put it somewhere in your PATH and type `zerotier-dns -h`.

## Todo
* Cache responses from the Zerotier API
* Optimize for speed
* Add all possible record-types
* DNS-Sec

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details

## Acknowledgments

* Most of the DNS-code was taken from the [dnsguide](https://github.com/EmilHernvall/dnsguide) written by [Emil Hernvall](https://github.com/EmilHer).
