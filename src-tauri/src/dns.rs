// User-level DNS filtering proxy. Listens on 127.0.0.1:53, forwards queries to
// an upstream resolver, and answers 0.0.0.0 for domains on the active blocklist.
//
// This is the ONLY mechanism Nivara uses to block websites — window-title
// matching cannot block anything and is not used for enforcement. The proxy is
// a normal user-level process; it only takes effect once the system resolver is
// pointed at 127.0.0.1 (an explicit, elevated one-time step). It is fully
// bypassable: change DNS back, use DoH/VPN, or kill this process. See CLAUDE.md.

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared, live-updatable blocklist of lowercase domains (e.g. "youtube.com").
pub type BlockList = Arc<Mutex<Vec<String>>>;

pub fn new_blocklist() -> BlockList {
    Arc::new(Mutex::new(Vec::new()))
}

/// Parse the first QNAME from a DNS query packet into a dotted lowercase string.
fn parse_qname(packet: &[u8]) -> Option<String> {
    if packet.len() < 12 {
        return None;
    }
    let mut pos = 12; // skip header
    let mut labels: Vec<String> = Vec::new();
    loop {
        if pos >= packet.len() {
            return None;
        }
        let len = packet[pos] as usize;
        if len == 0 {
            break;
        }
        if len & 0xC0 != 0 {
            return None; // compression pointer in a question is unexpected
        }
        pos += 1;
        if pos + len > packet.len() {
            return None;
        }
        let label = String::from_utf8_lossy(&packet[pos..pos + len]).to_lowercase();
        labels.push(label);
        pos += len;
    }
    if labels.is_empty() {
        None
    } else {
        Some(labels.join("."))
    }
}

/// True if `qname` equals or is a subdomain of any blocked domain.
fn is_blocked(qname: &str, blocklist: &[String]) -> bool {
    blocklist.iter().any(|b| {
        let b = b.trim().trim_start_matches("www.");
        !b.is_empty() && (qname == b || qname.ends_with(&format!(".{}", b)))
    })
}

/// QTYPE of the (single) question, or 0.
fn qtype(packet: &[u8]) -> u16 {
    let mut pos = 12;
    while pos < packet.len() {
        let len = packet[pos] as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        pos += 1 + len;
    }
    if pos + 2 <= packet.len() {
        u16::from_be_bytes([packet[pos], packet[pos + 1]])
    } else {
        0
    }
}

/// Build a "0.0.0.0" A-record answer (or NXDOMAIN for non-A) echoing the query.
fn build_block_response(query: &[u8]) -> Vec<u8> {
    // Find end of question section.
    let mut pos = 12;
    while pos < query.len() {
        let len = query[pos] as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        pos += 1 + len;
    }
    let q_end = (pos + 4).min(query.len()); // QTYPE(2) + QCLASS(2)
    let is_a = qtype(query) == 1;

    let mut resp = Vec::with_capacity(q_end + 16);
    resp.extend_from_slice(&query[0..2]); // ID
    // Flags: QR=1, RD copied, RA=1; RCODE 0 (A block) or 3 (NXDOMAIN).
    let rd = query[2] & 0x01;
    resp.push(0x80 | rd);
    resp.push(if is_a { 0x80 } else { 0x83 });
    resp.extend_from_slice(&query[4..6]); // QDCOUNT
    resp.extend_from_slice(&(if is_a { 1u16 } else { 0u16 }).to_be_bytes()); // ANCOUNT
    resp.extend_from_slice(&[0, 0, 0, 0]); // NS + AR counts
    resp.extend_from_slice(&query[12..q_end]); // original question

    if is_a {
        resp.extend_from_slice(&[0xC0, 0x0C]); // name pointer to question
        resp.extend_from_slice(&[0x00, 0x01]); // TYPE A
        resp.extend_from_slice(&[0x00, 0x01]); // CLASS IN
        resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x1E]); // TTL 30s
        resp.extend_from_slice(&[0x00, 0x04]); // RDLENGTH
        resp.extend_from_slice(&[0, 0, 0, 0]); // 0.0.0.0
    }
    resp
}

/// Forward a query to a list of upstream resolvers (in order, with one retry
/// each) and return the first response. None if all fail.
fn forward(query: &[u8], upstreams: &[String]) -> Option<Vec<u8>> {
    for up in upstreams {
        let addr = format!("{}:53", up);
        for _ in 0..2 {
            if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
                let _ = sock.set_read_timeout(Some(Duration::from_millis(1500)));
                if sock.send_to(query, &addr).is_ok() {
                    let mut rbuf = [0u8; 4096];
                    if let Ok((rn, _)) = sock.recv_from(&mut rbuf) {
                        return Some(rbuf[..rn].to_vec());
                    }
                }
            }
        }
    }
    None
}

/// Run the DNS proxy loop until the process exits. `log` receives status lines.
/// `upstream` may be a single resolver or comma-separated list; a public backup
/// (8.8.8.8) is always appended so forwarding stays reliable.
pub fn serve(blocklist: BlockList, upstream: String, log: impl Fn(&str)) {
    let socket = match UdpSocket::bind("127.0.0.1:53") {
        Ok(s) => s,
        Err(e) => {
            log(&format!("dns proxy bind 127.0.0.1:53 failed: {}", e));
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));

    let mut upstreams: Vec<String> = upstream
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for backup in ["1.1.1.1", "8.8.8.8"] {
        if !upstreams.iter().any(|u| u == backup) {
            upstreams.push(backup.to_string());
        }
    }
    log(&format!(
        "dns proxy listening on 127.0.0.1:53 (upstreams {})",
        upstreams.join(", ")
    ));

    let mut buf = [0u8; 4096];
    loop {
        let (n, client) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(_) => continue, // timeout — allows periodic exit checks by caller thread
        };
        let query = buf[..n].to_vec();
        let qname = parse_qname(&query).unwrap_or_default();
        let blocked = {
            let list = blocklist.lock().unwrap();
            !qname.is_empty() && is_blocked(&qname, &list)
        };

        if blocked {
            let resp = build_block_response(&query);
            let _ = socket.send_to(&resp, client);
            log(&format!("dns blocked: {}", qname));
            continue;
        }

        // Forward to upstreams on a worker thread so one slow lookup never
        // stalls other clients (keeps browsing responsive).
        let sock = match socket.try_clone() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let ups = upstreams.clone();
        std::thread::spawn(move || {
            if let Some(resp) = forward(&query, &ups) {
                let _ = sock.send_to(&resp, client);
            }
        });
    }
}
