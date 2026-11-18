//! Docker via API no socket Unix — sem chamar o CLI e sem acordar o daemon quando ele está parado.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const SOCK: &str = "/var/run/docker.sock";

#[derive(Clone, Debug)]
pub struct DPort {
    pub ip: String,
    pub public: u16,
    pub private: u16,
    pub proto: String,
}

#[derive(Clone, Debug)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: Vec<DPort>,
}

/// O docker.socket ativa o daemon ao primeiro acesso; só consultamos se ele já estiver rodando.
pub fn running() -> bool {
    Path::new("/var/run/docker.pid").exists() || Path::new("/run/docker.pid").exists()
}

fn request(method: &str, path: &str, timeout: Duration) -> Option<(u16, String)> {
    let mut s = UnixStream::connect(SOCK).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(s, "{method} {path} HTTP/1.0\r\nHost: docker\r\nContent-Length: 0\r\n\r\n").ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n")?;
    let code = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((code, body.to_string()))
}

pub fn list() -> Vec<Container> {
    if !running() {
        return vec![];
    }
    let Some((200, body)) = request("GET", "/containers/json", Duration::from_secs(2)) else {
        return vec![];
    };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&body) else {
        return vec![];
    };
    items
        .iter()
        .map(|c| Container {
            id: c["Id"].as_str().unwrap_or_default().to_string(),
            name: c["Names"][0].as_str().unwrap_or("?").trim_start_matches('/').to_string(),
            image: c["Image"].as_str().unwrap_or_default().to_string(),
            status: c["Status"].as_str().unwrap_or_default().to_string(),
            ports: c["Ports"]
                .as_array()
                .map(|ps| {
                    ps.iter()
                        .map(|p| DPort {
                            ip: p["IP"].as_str().unwrap_or("0.0.0.0").to_string(),
                            public: p["PublicPort"].as_u64().unwrap_or(0) as u16,
                            private: p["PrivatePort"].as_u64().unwrap_or(0) as u16,
                            proto: p["Type"].as_str().unwrap_or("tcp").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

pub fn stop(id: &str) -> bool {
    matches!(request("POST", &format!("/containers/{id}/stop"), Duration::from_secs(20)), Some((204 | 304, _)))
}

/// "ghcr.io/org/postgres:15-alpine" -> "postgres:15-alpine"
pub fn short_image(image: &str) -> String {
    image.rsplit('/').next().unwrap_or(image).to_string()
}
