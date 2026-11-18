//! Coleta de sockets e processos direto do /proc (sem depender do `ss`).
use procfs::net::TcpState;
use procfs::process::{all_processes, FDTarget, Process};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    pub fn as_str(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

/// Até onde a porta é alcançável, pelo endereço em que ela escuta.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Scope {
    Local,
    Specific,
    All,
}

pub fn scope_of(ip: &IpAddr) -> Scope {
    let ip = ip.to_canonical();
    if ip.is_loopback() {
        Scope::Local
    } else if ip.is_unspecified() {
        Scope::All
    } else {
        Scope::Specific
    }
}

#[derive(Clone, Debug)]
pub struct Listener {
    pub proto: Proto,
    pub port: u16,
    pub addrs: Vec<IpAddr>,
    pub scope: Scope,
    pub pid: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct Conn {
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub pid: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub cmdline: String,
    pub cwd: Option<PathBuf>,
    pub user: String,
    pub rss: u64,
    pub uptime_secs: u64,
    pub cpu_ticks: u64,
}

pub struct Snapshot {
    pub listeners: Vec<Listener>,
    pub conns: Vec<Conn>,
    pub procs: HashMap<i32, ProcInfo>,
}

pub fn canon(sa: SocketAddr) -> SocketAddr {
    SocketAddr::new(sa.ip().to_canonical(), sa.port())
}

pub fn load_users() -> HashMap<u32, String> {
    std::fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split(':');
            let name = it.next()?;
            let uid = it.nth(1)?.parse().ok()?;
            Some((uid, name.to_string()))
        })
        .collect()
}

fn system_uptime() -> f64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0)
}

pub fn snapshot(with_udp: bool, users: &HashMap<u32, String>) -> Snapshot {
    // inode do socket -> PID dono (só dá para ver processos do próprio usuário sem root)
    let mut owner: HashMap<u64, i32> = HashMap::new();
    let mut handles: HashMap<i32, Process> = HashMap::new();
    if let Ok(all) = all_processes() {
        for p in all.flatten() {
            let Ok(fds) = p.fd() else { continue };
            let mut has_socket = false;
            for fd in fds.flatten() {
                if let FDTarget::Socket(ino) = fd.target {
                    owner.insert(ino, p.pid);
                    has_socket = true;
                }
            }
            if has_socket {
                handles.insert(p.pid, p);
            }
        }
    }

    let mut groups: HashMap<(Proto, u16), Listener> = HashMap::new();
    let mut conns = Vec::new();
    let mut add = |proto: Proto, sa: SocketAddr, ino: u64| {
        let e = groups.entry((proto, sa.port())).or_insert(Listener {
            proto,
            port: sa.port(),
            addrs: vec![],
            scope: Scope::Local,
            pid: None,
        });
        let ip = sa.ip().to_canonical();
        if !e.addrs.contains(&ip) {
            e.addrs.push(ip);
        }
        let sc = scope_of(&ip);
        if sc > e.scope {
            e.scope = sc;
        }
        if e.pid.is_none() {
            e.pid = owner.get(&ino).copied();
        }
    };

    let mut tcp = procfs::net::tcp().unwrap_or_default();
    tcp.extend(procfs::net::tcp6().unwrap_or_default());
    for e in tcp {
        match e.state {
            TcpState::Listen => add(Proto::Tcp, e.local_address, e.inode),
            TcpState::Established => conns.push(Conn {
                local: canon(e.local_address),
                remote: canon(e.remote_address),
                pid: owner.get(&e.inode).copied(),
            }),
            _ => {}
        }
    }
    if with_udp {
        let mut udp = procfs::net::udp().unwrap_or_default();
        udp.extend(procfs::net::udp6().unwrap_or_default());
        for e in udp {
            // sockets "servidor": sem destino fixo e fora da faixa de portas efêmeras
            if e.remote_address.port() == 0 && e.local_address.port() < 32768 {
                add(Proto::Udp, e.local_address, e.inode);
            }
        }
    }

    let mut listeners: Vec<Listener> = groups.into_values().collect();
    listeners.sort_by_key(|l| (l.port, l.proto));

    let wanted: HashSet<i32> = listeners.iter().filter_map(|l| l.pid).collect();
    let tps = procfs::ticks_per_second() as f64;
    let page = procfs::page_size();
    let up = system_uptime();
    let mut procs = HashMap::new();
    for pid in wanted {
        let Some(p) = handles.get(&pid) else { continue };
        let Ok(st) = p.stat() else { continue };
        let cmdline = p.cmdline().map(|v| v.join(" ")).unwrap_or_default();
        let user = p
            .uid()
            .ok()
            .map(|u| users.get(&u).cloned().unwrap_or_else(|| u.to_string()))
            .unwrap_or_default();
        // o kernel corta o "comm" em 15 caracteres ("next-server (v1"): usa o nome do comando
        let name = if st.comm.len() >= 15 {
            cmdline
                .split_whitespace()
                .next()
                .and_then(|c| c.rsplit('/').next())
                .filter(|c| !c.is_empty())
                .unwrap_or(&st.comm)
                .to_string()
        } else {
            st.comm.clone()
        };
        procs.insert(
            pid,
            ProcInfo {
                pid,
                name,
                cmdline,
                cwd: p.cwd().ok(),
                user,
                rss: st.rss * page,
                uptime_secs: (up - st.starttime as f64 / tps).max(0.0) as u64,
                cpu_ticks: st.utime + st.stime,
            },
        );
    }

    Snapshot { listeners, conns, procs }
}
