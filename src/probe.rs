//! Teste HTTP em segundo plano: status, tempo de resposta, servidor/framework e <title>.
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum Probe {
    Pending,
    Http { status: u16, reason: String, ms: u128, ctype: Option<String>, server: Option<String>, title: Option<String> },
    NotHttp,
    Closed,
}

type Results = Arc<Mutex<HashMap<u16, (Instant, Probe)>>>;

pub struct Prober {
    tx: Sender<u16>,
    results: Results,
}

impl Prober {
    pub fn new() -> Self {
        let (tx, rx) = channel::<u16>();
        let results: Results = Arc::default();
        let r = results.clone();
        thread::spawn(move || {
            for port in rx {
                let p = probe(port);
                r.lock().unwrap().insert(port, (Instant::now(), p));
            }
        });
        Self { tx, results }
    }

    pub fn request(&self, port: u16) {
        let mut m = self.results.lock().unwrap();
        if matches!(m.get(&port), Some((_, Probe::Pending))) {
            return;
        }
        m.insert(port, (Instant::now(), Probe::Pending));
        drop(m);
        let _ = self.tx.send(port);
    }

    /// Define um resultado sem testar (modo demonstração).
    pub fn set(&self, port: u16, p: Probe) {
        self.results.lock().unwrap().insert(port, (Instant::now(), p));
    }

    pub fn get(&self, port: u16) -> Option<(Instant, Probe)> {
        self.results.lock().unwrap().get(&port).cloned()
    }
}

fn probe(port: u16) -> Probe {
    let start = Instant::now();
    let targets = [SocketAddr::from((Ipv4Addr::LOCALHOST, port)), SocketAddr::from((Ipv6Addr::LOCALHOST, port))];
    let Some(mut s) = targets.iter().find_map(|t| TcpStream::connect_timeout(t, Duration::from_millis(700)).ok())
    else {
        return Probe::Closed;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(4000)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
    let req = format!(
        "GET / HTTP/1.1\r\nHost: localhost:{port}\r\nUser-Agent: portkyu/1.0\r\nAccept: text/html,*/*\r\nConnection: close\r\n\r\n"
    );
    if s.write_all(req.as_bytes()).is_err() {
        return Probe::NotHttp;
    }
    let mut buf = Vec::with_capacity(16 * 1024);
    let mut chunk = [0u8; 8192];
    let mut first_byte = None;
    loop {
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                first_byte.get_or_insert_with(|| start.elapsed().as_millis());
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 256 * 1024 || contains_ci(&buf, b"</title>") {
                    break;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&buf);
    if !text.starts_with("HTTP/") {
        return Probe::NotHttp;
    }
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let mut status_line = lines.next().unwrap_or_default().splitn(3, ' ');
    status_line.next();
    let status = status_line.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let reason = status_line.next().unwrap_or_default().to_string();
    let (mut ctype, mut server, mut powered) = (None, None, None);
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let v = v.trim().to_string();
            match k.trim().to_ascii_lowercase().as_str() {
                "content-type" => ctype = v.split(';').next().map(str::to_string),
                "server" => server = Some(v),
                "x-powered-by" => powered = Some(v),
                _ => {}
            }
        }
    }
    let server = match (powered, server) {
        (Some(p), Some(s)) => Some(format!("{p} · {s}")),
        (p, s) => p.or(s),
    };
    Probe::Http { status, reason, ms: first_byte.unwrap_or(0), ctype, server, title: extract_title(body) }
}

fn contains_ci(h: &[u8], n: &[u8]) -> bool {
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

fn extract_title(body: &str) -> Option<String> {
    let low = body.to_ascii_lowercase();
    let i = low.find("<title")?;
    let s = i + low[i..].find('>')? + 1;
    let e = s + low[s..].find("</title")?;
    let t = body[s..e].split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'");
    (!t.is_empty()).then_some(t)
}
