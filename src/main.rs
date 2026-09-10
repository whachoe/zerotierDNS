#[macro_use]
extern crate reqwest;
extern crate json;
extern crate clap;

use clap::{Arg, App};
use reqwest::header::AUTHORIZATION;
use json::JsonValue;

use std::net::UdpSocket;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::str::FromStr;
use std::time::{Duration, Instant};

// Network calls (ZeroTier API request, proxy DNS round-trip) must never block
// longer than this, otherwise the single-threaded main loop stalls and stops
// reading further queries off the listening socket.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);

// How long a fetched ZeroTier member list stays valid before we hit the API again.
const ZEROTIER_CACHE_DURATION: Duration = Duration::from_secs(1440 * 60);

mod BytePacketBuffer;
mod DnsHeader;
mod DnsPacket;
mod DnsQuestion;
mod DnsRecord;
mod QueryType;
mod ResultCode;

// If you want to use this server as a proxy-dns-server to an upstream link, use this function
#[allow(dead_code)]
fn proxy_lookup(qname: &str, qtype: QueryType::QueryType, server: (&str, u16)) -> std::io::Result<DnsPacket::DnsPacket> {
    println!("proxy_lookup: Received query: {:?}", qname);

    let socket = UdpSocket::bind(("0.0.0.0", 43210))?;
    socket.set_read_timeout(Some(NETWORK_TIMEOUT))?;

    let mut packet = DnsPacket::DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet.questions.push(DnsQuestion::DnsQuestion::new(qname.to_string(), qtype));

    let mut req_buffer = BytePacketBuffer::BytePacketBuffer::new();
    packet.write(&mut req_buffer).unwrap();
    socket.send_to(&req_buffer.buf[0..req_buffer.pos], server)?;

    let mut res_buffer = BytePacketBuffer::BytePacketBuffer::new();
    socket.recv_from(&mut res_buffer.buf)?;

    DnsPacket::DnsPacket::from_buffer(&mut res_buffer)
}

// A previously-fetched ZeroTier member list (name -> IP), plus when it was fetched.
struct ZerotierCache {
    fetched_at: Instant,
    devices: Vec<(String, String)>
}

// Lookup the qname in zerotier api and return the IP
fn lookup(qname: &str, qtype: QueryType::QueryType, zerotier_token: &str, zerotier_network_id: &str, custom_domain: &str, cache: &mut Option<ZerotierCache>) -> std::result::Result<DnsPacket::DnsPacket, &'static str> {
    println!("lookup: Sending query to zerotier: {:?}", qname);

    // ZeroTier device names don't include the custom domain hosts are served
    // under (e.g. "cjmini" vs. the queried "cjmini.localdomain"), so strip it
    // before matching against the API's device list.
    let suffix = format!(".{}", custom_domain.to_lowercase());
    let device_name = qname.strip_suffix(&suffix).unwrap_or(qname);

    let needs_refresh = match cache {
        Some(c) => c.fetched_at.elapsed() >= ZEROTIER_CACHE_DURATION,
        None => true
    };

    if needs_refresh {
        println!("lookup: ZeroTier cache missing or expired, refreshing from API");

        let zerotier_url = format!("https://my.zerotier.com/api/network/{network_id}/member", network_id = zerotier_network_id);
        let auth_header = format!("Bearer {token}", token = zerotier_token);

        let client = match reqwest::Client::builder().timeout(NETWORK_TIMEOUT).build() {
            Ok(c) => c,
            Err(_) => return Err("Failed to build HTTP client"),
        };

        let mut response = match client.get(&zerotier_url).header(AUTHORIZATION, auth_header).send() {
            Ok(r) => r,
            Err(_) => return Err("Failed to reach ZeroTier API"),
        };

        let response_content = match response.text() {
            Ok(t) => t,
            Err(_) => return Err("Failed to read ZeroTier API response"),
        };

        // Parse the json
        let parsed = match json::parse(&response_content.to_string()) {
            Ok(p) => p,
            Err(_) => return Err("Failed to parse ZeroTier API response"),
        };

        let mut devices = Vec::new();
        if parsed.is_array() {
            for device in parsed.members() {
                if device.is_object() {
                    let name = device["name"].to_string();
                    let ip = device["config"]["ipAssignments"][0].to_string();
                    devices.push((name, ip));
                }
            }
        }

        *cache = Some(ZerotierCache { fetched_at: Instant::now(), devices });
    } else {
        println!("lookup: Using cached ZeroTier device list");
    }

    let devices = &cache.as_ref().unwrap().devices;
    let mut ip = String::new();
    let mut found = false;

    for (name, device_ip) in devices {
        println!("Found: {} -> {}", name, device_ip);

        if name.eq(device_name) {
            println!("Matched: {} -> {}", name, device_ip);
            ip = device_ip.clone();
            found = true;
            break;
        }
    }

    let mut packet = DnsPacket::DnsPacket::new();

    if found {
        // We're authoritative for this name, so answer even if there's no
        // record of the requested type (NODATA) rather than falling through
        // to the proxy or returning a record whose type doesn't match the
        // question - either of which resolvers correctly reject/mishandle.
        if qtype == QueryType::QueryType::A {
            let record = DnsRecord::DnsRecord::A {
                domain: qname.to_string(),
                addr: Ipv4Addr::from_str(&ip).unwrap(),
                ttl: 3600
            };
            packet.header.answers = 1;
            packet.answers.push(record);
        }

        return Ok(packet)
    }

    Err("Host not found")
}

fn main() {
    let matches = App::new("ZerotierDNS")
                        .version("1.0.0")
                        .author("Whachoe <whachoe@gmail.com>")
                        .about("Dns-server for zerotier networks. Resolves names of devices to their IP")
                        .arg(Arg::with_name("zerotier-token")
                            .short("t")
                            .long("token")
                            .value_name("YOUR-ZEROTIER-API-TOKEN")
                            .help("See https://my.zerotier.com/ to create one")
                            .required(true)
                            .takes_value(true))
                        .arg(Arg::with_name("zerotier-network-id")
                            .short("n")
                            .long("network")
                            .value_name("YOUR-ZEROTIER-NETWORK-ID")
                            .help("The Network ID of your zerotier-network")
                            .required(true)
                            .takes_value(true))
                        .arg(Arg::with_name("custom-domain")
                            .short("d")
                            .long("domain")
                            .value_name("example.com")
                            .help("The domain you want your hosts to be resolved under")
                            .takes_value(true)
                            .default_value("localdomain"))
                        .arg(Arg::with_name("bind-address")
                            .short("b")
                            .long("bind")
                            .value_name("IP to bind on")
                            .help("If left out, the app will bind on all available IP's. It's more secure to bind the IP of your local zerotier-client.")
                            .takes_value(true)
                            .required(false))
                        .arg(Arg::with_name("proxy-server")
                            .short("p")
                            .long("proxy")
                            .value_name("IP of Proxy")
                            .help("IP of the server to proxy requests to in case we did not find a match.")
                            .takes_value(true)
                            .required(false)
                            .default_value("8.8.8.8"))

                        .get_matches();

    let zerotier_token = matches.value_of("zerotier-token").unwrap();
    let zerotier_network_id = matches.value_of("zerotier-network-id").unwrap();
    let bind_address = matches.value_of("bind-address").unwrap_or("0.0.0.0");
    let custom_domain = matches.value_of("custom-domain").unwrap_or("localdomain");
    let proxy_ip = matches.value_of("proxy-server").unwrap_or("8.8.8.8");
    let socket = UdpSocket::bind((bind_address, 53)).unwrap();
    let mut zerotier_cache: Option<ZerotierCache> = None;

    println!("Started Zerotier-DNS on {}:53", bind_address);

    // Main event loop
    loop {
        // Blocking read from the socket
        let mut req_buffer = BytePacketBuffer::BytePacketBuffer::new();
        let (_, src) = match socket.recv_from(&mut req_buffer.buf) {
            Ok(x) => x,
            Err(e) => {
                println!("Failed to read from UDP socket: {:?}", e);
                continue;
            }
        };

        // Parse the packet
        let request = match DnsPacket::DnsPacket::from_buffer(&mut req_buffer) {
            Ok(x) => x,
            Err(e) => {
                println!("Failed to parse query packet: {:?}", e);
                continue;
            }
        };

        // Prepare the response DnsPacket
        let mut packet = DnsPacket::DnsPacket::new();
        packet.header.id = request.header.id;
        packet.header.recursion_desired = true;
        packet.header.recursion_available = true;
        packet.header.response = true;

        // If there's no question-section in the request, notify the caller
        if request.questions.is_empty() {
            packet.header.rescode = ResultCode::ResultCode::FORMERR;
        } else {
            let question = &request.questions[0];
            println!("Received query: {:?}", question);

            // Forward query to the target server and parse the answer
            if let Ok(result) = lookup(&question.name, question.qtype, zerotier_token, zerotier_network_id, custom_domain, &mut zerotier_cache) {
                packet.questions.push(question.clone());
                packet.header.rescode = result.header.rescode;

                for rec in result.answers {
                    packet.answers.push(rec);
                }

                for rec in result.authorities {
                    packet.authorities.push(rec);
                }

                for rec in result.resources {
                    packet.resources.push(rec);
                }
            } else {
                let server = (proxy_ip, 53);
                if let Ok(result) = proxy_lookup(&question.name, question.qtype, server) {
                    packet.questions.push(question.clone());
                    packet.header.rescode = result.header.rescode;

                    for rec in result.answers {
                        packet.answers.push(rec);
                    }

                    for rec in result.authorities {
                        packet.authorities.push(rec);
                    }

                    for rec in result.resources {
                        packet.resources.push(rec);
                    }
                } else {
                    packet.header.rescode = ResultCode::ResultCode::SERVFAIL;
                }
            }
        }

        let mut res_buffer = BytePacketBuffer::BytePacketBuffer::new();
        match packet.write(&mut res_buffer) {
            Ok(_) => {},
            Err(e) => {
                println!("Failed to encode response packet: {:?}", e);
                continue;
            }
        };

        let len = res_buffer.pos();
        let data = match res_buffer.get_range(0, len) {
            Ok(x) => x,
            Err(e) => {
                println!("Failed to retrieve response buffer: {:?}", e);
                continue;
            }
        };

        match socket.send_to(data, src) {
            Ok(x) => {},
            Err(e) => {
                println!("Failed to send response: {:?}", e);
                continue;
            }
        };
    }
}
