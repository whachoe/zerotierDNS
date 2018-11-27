use std::net::UdpSocket;

mod BytePacketBuffer;
mod DnsHeader;
mod DnsPacket;
mod DnsQuestion;
mod DnsRecord;
mod QueryType;
mod ResultCode;

fn main() {
    let qname = "google.com";
    let qtype = QueryType::QueryType::A;
    let server = ("8.8.8.8", 53);

    let socket = UdpSocket::bind(("0.0.0.0", 43210)).unwrap();

    let mut packet = DnsPacket::DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet.questions.push(DnsQuestion::DnsQuestion::new(qname.to_string(), qtype));

    let mut req_buffer = BytePacketBuffer::BytePacketBuffer::new();
    packet.write(&mut req_buffer).unwrap();

    socket.send_to(&req_buffer.buf[0..req_buffer.pos], server).unwrap();

    let mut res_buffer = BytePacketBuffer::BytePacketBuffer::new();
    socket.recv_from(&mut res_buffer.buf).unwrap();

    let packet = DnsPacket::DnsPacket::from_buffer(&mut res_buffer).unwrap();
    println!("{:?}", packet.header);

    for q in packet.questions {
        println!("{:?}", q);
    }
    for rec in packet.answers {
        println!("{:?}", rec);
    }
    for rec in packet.authorities {
        println!("{:?}", rec);
    }
    for rec in packet.resources {
        println!("{:?}", rec);
    }
}
