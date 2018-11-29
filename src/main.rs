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
use std::io::Read;
use std::io::{Result};
use std::str::FromStr;

mod BytePacketBuffer;
mod DnsHeader;
mod DnsPacket;
mod DnsQuestion;
mod DnsRecord;
mod QueryType;
mod ResultCode;

// If you want to use this server as a proxy-dns-server to an upstream link, use this function
#[allow(dead_code)]
fn proxy_lookup(qname: &str, qtype: QueryType::QueryType) -> Result<DnsPacket::DnsPacket> {
    let server = ("8.8.8.8", 53);
    let socket = UdpSocket::bind(("0.0.0.0", 43210))?;

    let mut packet = DnsPacket::DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet.questions.push(DnsQuestion::DnsQuestion::new(qname.to_string(), qtype));

    let mut req_buffer = BytePacketBuffer::BytePacketBuffer::new();
    packet.write(&mut req_buffer).unwrap();
    socket.send_to(&req_buffer.buf[0..req_buffer.pos], server)?;

    let mut res_buffer = BytePacketBuffer::BytePacketBuffer::new();
    socket.recv_from(&mut res_buffer.buf).unwrap();

    DnsPacket::DnsPacket::from_buffer(&mut res_buffer)
}

// Lookup the qname in zerotier api and return the IP
// todo: Implement local caching of the API-response
#[allow(dead_code)]
fn lookup(qname: &str, zerotier_token: &str, zerotier_network_id: &str) -> Result<DnsPacket::DnsPacket> {
    let zerotier_url = format!("https://my.zerotier.com/api/network/{network_id}/member", network_id = zerotier_network_id);
    let auth_header = format!("Bearer {token}", token = zerotier_token);

    let client = reqwest::Client::new();
    let mut response = client.get(&zerotier_url).header(AUTHORIZATION, auth_header).send().unwrap();
    let response_content = response.text().unwrap();

    // println!("Response: {}", response_content);

    // Parse the json
    let parsed = json::parse(&response_content.to_string()).unwrap();
    let mut name = String::new();
    let mut ip = String::new();
    let mut found = false;

    if parsed.is_array() {
        for device in parsed.members() {
            if device.is_object() {
                name = device["name"].to_string();
                ip = device["config"]["ipAssignments"][0].to_string();
                println!("Found: {} -> {}", name, ip);

                if name.eq(qname) {
                    found = true;
                    break;
                }
            }
        }
    }

    let mut packet = DnsPacket::DnsPacket::new();

    if found {
        let record = DnsRecord::DnsRecord::A {
            domain: name,
            addr: Ipv4Addr::from_str(&ip).unwrap(),
            ttl: 3600
        };
        packet.header.answers = 1;
        packet.answers.push(record);
    }

    Ok(packet)
}

fn main() {
    let matches = App::new("ZerotierDNS")
                        .version("1.0.0")
                        .author("Whachoe <whachoe@gmail.com>")
                        .about("Dns-server for zerotier networks. Resolves names of clients to their IP")
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
                        .arg(Arg::with_name("bind-address")
                            .short("b")
                            .long("bind")
                            .value_name("IP to bind on")
                            .help("If left out, the app will bind on all available IP's. It's more secure to bind the IP of your local zerotier-client.")
                            .takes_value(true)
                            .required(false))
                        .get_matches();

    let zerotier_token = matches.value_of("zerotier-token").unwrap();
    let zerotier_network_id = matches.value_of("zerotier-network-id").unwrap();
    let bind_address = matches.value_of("bind-address").unwrap_or("0.0.0.0");
    let socket = UdpSocket::bind((bind_address, 53)).unwrap();

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
            if let Ok(result) = lookup(&question.name, zerotier_token, zerotier_network_id) {
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
