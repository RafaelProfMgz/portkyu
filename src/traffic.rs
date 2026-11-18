//! Bytes trafegados por conexão (tcp_info do kernel via `ss -ti`), usado para calcular a taxa.
use crate::collect::canon;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::Command;

#[derive(Clone, Copy, Debug, Default)]
pub struct ConnStat {
    pub rx: u64,
    pub tx: u64,
    pub rtt_ms: Option<f64>,
}

pub type Key = (SocketAddr, SocketAddr);

fn parse_addr(s: &str) -> Option<SocketAddr> {
    // remove o escopo de interface: "127.0.0.53%lo:53", "[fe80::1%wlan0]:22"
    let s = match s.find('%') {
        Some(i) => {
            let rest = &s[i..];
            let end = rest.find(']').or_else(|| rest.rfind(':')).unwrap_or(rest.len());
            format!("{}{}", &s[..i], &rest[end..])
        }
        None => s.to_string(),
    };
    s.parse::<SocketAddr>().ok().map(canon)
}

pub fn sample() -> HashMap<Key, ConnStat> {
    let Ok(out) = Command::new("ss").args(["-HtinO", "state", "established"]).output() else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut map = HashMap::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 {
            continue;
        }
        let (Some(l), Some(r)) = (parse_addr(t[2]), parse_addr(t[3])) else { continue };
        let mut st = ConnStat::default();
        for tok in &t[4..] {
            if let Some(v) = tok.strip_prefix("bytes_acked:") {
                st.tx = v.parse().unwrap_or(0);
            } else if let Some(v) = tok.strip_prefix("bytes_received:") {
                st.rx = v.parse().unwrap_or(0);
            } else if let Some(v) = tok.strip_prefix("rtt:") {
                st.rtt_ms = v.split('/').next().and_then(|x| x.parse().ok());
            }
        }
        map.insert((l, r), st);
    }
    map
}
