//! Estado do app: atualização periódica, classificação, eventos e ações do usuário.
use crate::collect::{self, Conn, Listener, ProcInfo, Proto, Scope};
use crate::docker::{self, Container};
use crate::firewall::{self, Exposure, Firewall};
use crate::probe::{Probe, Prober};
use crate::project::{Project, ProjectCache};
use crate::theme::*;
use crate::traffic::{self, ConnStat, Key};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    Dev,
    Database,
    Tool,
    Other,
    System,
}

impl Kind {
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Dev => I_DEV,
            Kind::Database => I_DB,
            Kind::Tool => I_TOOL,
            Kind::Other => I_OTHER,
            Kind::System => I_SYS,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    All,
    Dev,
    Db,
    Docker,
    Exposed,
}

pub const VIEWS: [(View, &str); 5] = [
    (View::All, "Tudo"),
    (View::Dev, "Dev"),
    (View::Db, "Bancos"),
    (View::Docker, "Docker"),
    (View::Exposed, "Expostas"),
];

pub struct Entry {
    pub proto: Proto,
    pub port: u16,
    pub addrs: Vec<IpAddr>,
    pub pid: Option<i32>,
    pub kind: Kind,
    pub title: String,
    pub stack: Option<String>,
    pub service: &'static str,
    pub proc_: Option<ProcInfo>,
    pub project: Option<Project>,
    pub container: Option<Container>,
    pub conns: usize,
    pub rate: f64,
    pub exposure: Exposure,
}

impl Entry {
    pub fn key(&self) -> (Proto, u16) {
        (self.proto, self.port)
    }
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

/// Tom de uma mensagem de status (cada interface escolhe a cor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Ok,
    Info,
    Warn,
    Err,
    Muted,
}

pub struct LogEvent {
    pub time: String,
    pub opened: bool,
    pub text: String,
}

#[derive(Clone)]
pub enum KillTarget {
    Pid(i32),
    Container { id: String, name: String },
}

pub enum Modal {
    None,
    Help,
    Confirm { label: String, target: KillTarget },
}

pub struct App {
    pub rows: Vec<Entry>,
    pub visible: Vec<usize>,
    pub sel: Option<usize>,
    sel_key: Option<(Proto, u16)>,
    pub view: View,
    pub filter: String,
    pub filtering: bool,
    pub show_udp: bool,
    pub events: VecDeque<LogEvent>,
    known: HashMap<(Proto, u16), String>,
    first: bool,
    pub hist: HashMap<(Proto, u16), VecDeque<u64>>,
    prev_bytes: HashMap<Key, (u64, u64)>,
    pub conn_stats: HashMap<Key, ConnStat>,
    pub conn_rate: HashMap<Key, f64>,
    pub conns: Vec<Conn>,
    cpu_prev: HashMap<i32, u64>,
    pub cpu: HashMap<i32, f64>,
    pub prober: Prober,
    probed: HashSet<u16>,
    projects: ProjectCache,
    users: HashMap<u32, String>,
    pub fw: Firewall,
    pub modal: Modal,
    pub status: Option<(String, Instant, Tone)>,
    pub is_root: bool,
    pub lan_ip: Option<IpAddr>,
    pub last_refresh: Instant,
    pub quit: bool,
    /// Modo demonstração: dados fictícios, ações desativadas (para capturas de tela e testes de interface).
    pub demo: bool,
    demo_tick: u64,
}

const NON_HTTP_PORTS: &[u16] = &[22, 25, 53, 139, 445, 631, 1716, 4369, 5353];

fn describe(port: u16) -> &'static str {
    match port {
        21 => "ftp",
        22 => "ssh",
        25 | 587 => "smtp",
        53 => "dns",
        80 => "http",
        443 => "https",
        139 | 445 => "samba",
        631 => "cups · impressão",
        1234 => "lm studio",
        1714..=1764 => "kde connect",
        3000..=3003 => "dev server",
        3306 | 33060 => "mysql",
        4000 | 4001 => "dev server",
        4173 => "vite preview",
        4200 => "angular",
        4369 => "epmd · erlang",
        5000 => "dev server",
        5173 | 5174 => "vite",
        5353 => "mdns",
        5432 => "postgres",
        5555 => "prisma studio",
        6006 => "storybook",
        6333 | 6334 => "qdrant",
        6379 => "redis",
        7700 => "meilisearch",
        8000 | 8001 => "api dev",
        8080 | 8081 => "http alt",
        8787 => "wrangler",
        8888 => "jupyter",
        9000 => "minio · php-fpm",
        9229 => "node debugger",
        9200 => "elasticsearch",
        11211 => "memcached",
        11434 => "ollama",
        27017 => "mongodb",
        41641 => "tailscale",
        54321..=54324 => "supabase",
        _ => "",
    }
}

fn classify(port: u16, name: Option<&str>, image: Option<&str>, has_project: bool) -> Kind {
    const DB_WORDS: &[&str] = &[
        "postgres",
        "mysql",
        "mariadb",
        "redis",
        "mongo",
        "qdrant",
        "meili",
        "clickhouse",
        "valkey",
        "elasticsearch",
        "opensearch",
        "memcached",
        "neo4j",
        "cassandra",
    ];
    const DB_PORTS: &[u16] = &[5432, 3306, 33060, 6379, 27017, 6333, 6334, 7700, 9200, 11211, 8123];
    const TOOL_PORTS: &[u16] = &[9229, 5555, 54323, 6006, 8888, 1234, 11434];
    const TOOL_NAMES: &[&str] = &["ollama", "lm-studio", "codium", "cursor", "code", "prisma", "jupyter", "claude"];
    const SYS_NAMES: &[&str] = &[
        "systemd-resolve",
        "cupsd",
        "kdeconnectd",
        "avahi-daemon",
        "sshd",
        "smbd",
        "nmbd",
        "tailscaled",
        "chrome",
        "chromium",
        "firefox",
        "spotify",
        "dnsmasq",
        "rpcbind",
        "cups-browsed",
        "epmd",
        "systemd",
        "NetworkManager",
    ];
    const DEV_NAMES: &[&str] = &[
        "node",
        "bun",
        "deno",
        "python",
        "python3",
        "uvicorn",
        "gunicorn",
        "ruby",
        "php",
        "go",
        "java",
        "beam.smp",
        "cargo",
        "air",
        "next-server",
        "vite",
        "esbuild",
        "dotnet",
        "dart",
        "flutter",
        "npm",
        "pnpm",
        "workerd",
    ];
    let name = name.unwrap_or("");
    if let Some(img) = image {
        let img = img.to_lowercase();
        return if DB_WORDS.iter().any(|w| img.contains(w)) { Kind::Database } else { Kind::Dev };
    }
    if DB_WORDS.iter().any(|w| name.starts_with(w)) || (name.is_empty() && DB_PORTS.contains(&port)) {
        return Kind::Database;
    }
    if TOOL_PORTS.contains(&port) || TOOL_NAMES.contains(&name) {
        return Kind::Tool;
    }
    if SYS_NAMES.iter().any(|n| name.starts_with(n)) {
        return Kind::System;
    }
    if has_project
        || DEV_NAMES.iter().any(|n| name == *n || name.starts_with(&format!("{n} ")))
        || name.starts_with("next-server")
        || matches!(port, 3000..=3010 | 4000..=4010 | 4173 | 4200 | 5000 | 5173 | 5174 | 8000 | 8080 | 8081 | 8787)
    {
        return Kind::Dev;
    }
    if port < 1024 || name.is_empty() {
        return Kind::System;
    }
    Kind::Other
}

fn lan_ip() -> Option<IpAddr> {
    // connect() em UDP não envia nada: só pergunta ao kernel qual IP local seria usado
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    s.connect((Ipv4Addr::new(1, 1, 1, 1), 80)).ok()?;
    Some(s.local_addr().ok()?.ip())
}

fn in_path(cmd: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file())).unwrap_or(false)
}

pub fn spawn_detached(cmd: &str, args: &[&str]) -> bool {
    match Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(mut c) => {
            thread::spawn(move || {
                let _ = c.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Abre um terminal (o primeiro disponível) na pasta `dir`, opcionalmente rodando `cmd`.
pub fn open_in_terminal(dir: Option<&std::path::Path>, title: &str, cmd: &[&str]) -> bool {
    let mut candidates: Vec<String> = vec![];
    if let Ok(t) = std::env::var("TERMINAL") {
        candidates.push(t);
    }
    for t in [
        "ghostty",
        "kitty",
        "wezterm",
        "alacritty",
        "konsole",
        "gnome-terminal",
        "kgx",
        "foot",
        "xfce4-terminal",
        "tilix",
        "xterm",
    ] {
        candidates.push(t.into());
    }
    let Some(term) = candidates.into_iter().find(|t| in_path(t)) else { return false };
    let mut args: Vec<String> = vec![];
    let title_arg = |flag: &str| format!("{flag}{title}");
    match term.as_str() {
        "ghostty" => args.push(title_arg("--title=")),
        "kitty" | "alacritty" => {
            args.push("--title".into());
            args.push(title.into());
        }
        _ => {}
    }
    if !cmd.is_empty() {
        match term.as_str() {
            "gnome-terminal" | "kgx" | "tilix" => args.push("--".into()),
            "wezterm" => {
                args.push("start".into());
                args.push("--".into());
            }
            "kitty" | "foot" => {}
            _ => args.push("-e".into()),
        }
        args.extend(cmd.iter().map(|s| s.to_string()));
    }
    let mut c = Command::new(&term);
    c.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
    if let Some(d) = dir {
        c.current_dir(d);
    }
    match c.spawn() {
        Ok(mut child) => {
            thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Caminho do próprio executável (dentro de um AppImage, o arquivo .AppImage).
pub fn self_exe() -> String {
    std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_else(|| "portkyu".into())
}

impl App {
    pub fn new(show_udp: bool, initial: Option<u16>) -> Self {
        Self {
            rows: vec![],
            visible: vec![],
            sel: None,
            sel_key: initial.map(|p| (Proto::Tcp, p)),
            view: View::All,
            filter: String::new(),
            filtering: false,
            show_udp,
            events: VecDeque::new(),
            known: HashMap::new(),
            first: true,
            hist: HashMap::new(),
            prev_bytes: HashMap::new(),
            conn_stats: HashMap::new(),
            conn_rate: HashMap::new(),
            conns: vec![],
            cpu_prev: HashMap::new(),
            cpu: HashMap::new(),
            prober: Prober::new(),
            probed: HashSet::new(),
            projects: ProjectCache::default(),
            users: collect::load_users(),
            fw: firewall::load(),
            modal: Modal::None,
            status: None,
            is_root: unsafe { libc::geteuid() } == 0,
            lan_ip: lan_ip(),
            last_refresh: Instant::now() - Duration::from_secs(1),
            quit: false,
            demo: false,
            demo_tick: 0,
        }
    }

    // ─── Atualização ────────────────────────────────────────────────────────
    pub fn refresh(&mut self) {
        if self.demo {
            return self.refresh_demo();
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_refresh).as_secs_f64().max(0.2);
        self.last_refresh = now;

        let snap = collect::snapshot(self.show_udp, &self.users);
        let containers = docker::list();
        self.fw = firewall::load();

        let stats = traffic::sample();
        self.conn_rate = stats
            .iter()
            .filter_map(|(k, s)| {
                let (prx, ptx) = self.prev_bytes.get(k)?;
                Some((*k, (s.rx.saturating_sub(*prx) + s.tx.saturating_sub(*ptx)) as f64 / dt))
            })
            .collect();
        self.prev_bytes = stats.iter().map(|(k, s)| (*k, (s.rx, s.tx))).collect();
        self.conn_stats = stats;

        // portas publicadas por containers
        let mut dmap: HashMap<(Proto, u16), usize> = HashMap::new();
        let mut dscope: HashMap<(Proto, u16), (Scope, Vec<IpAddr>)> = HashMap::new();
        for (i, c) in containers.iter().enumerate() {
            for p in c.ports.iter().filter(|p| p.public != 0) {
                let proto = if p.proto == "udp" { Proto::Udp } else { Proto::Tcp };
                dmap.insert((proto, p.public), i);
                let ip: IpAddr = p.ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
                let e = dscope.entry((proto, p.public)).or_insert((Scope::Local, vec![]));
                e.1.push(ip);
                e.0 = e.0.max(collect::scope_of(&ip));
            }
        }
        let mut listeners = snap.listeners;
        for (k, (sc, ips)) in dscope {
            if !listeners.iter().any(|l| (l.proto, l.port) == k) && (k.0 == Proto::Tcp || self.show_udp) {
                listeners.push(Listener { proto: k.0, port: k.1, addrs: ips, scope: sc, pid: None });
            }
        }

        // CPU %
        let tps = procfs::ticks_per_second() as f64;
        self.cpu = snap
            .procs
            .iter()
            .filter_map(|(pid, p)| {
                let prev = self.cpu_prev.get(pid)?;
                Some((*pid, p.cpu_ticks.saturating_sub(*prev) as f64 / tps / dt * 100.0))
            })
            .collect();
        self.cpu_prev = snap.procs.iter().map(|(pid, p)| (*pid, p.cpu_ticks)).collect();

        let mut rows = Vec::with_capacity(listeners.len());
        for l in listeners {
            let proc_ = l.pid.and_then(|p| snap.procs.get(&p).cloned());
            let project = proc_.as_ref().and_then(|p| p.cwd.clone()).and_then(|c| self.projects.get(&c));
            let container = dmap.get(&(l.proto, l.port)).map(|&i| containers[i].clone());
            let kind = classify(
                l.port,
                proc_.as_ref().map(|p| p.name.as_str()),
                container.as_ref().map(|c| c.image.as_str()),
                project.is_some(),
            );
            let service = describe(l.port);
            let title = if let Some(c) = &container {
                c.name.clone()
            } else if let Some(p) = &project {
                p.name.clone()
            } else if let Some(p) = &proc_ {
                p.name.clone()
            } else if self.is_root {
                "?".into()
            } else {
                "(root)".into()
            };
            let stack = if let Some(c) = &container {
                Some(docker::short_image(&c.image))
            } else if let Some(p) = &project {
                Some(p.stack.clone().unwrap_or_else(|| p.lang.to_string()))
            } else {
                (!service.is_empty()).then(|| service.to_string())
            };
            let (mut conns, mut rate) = (0, 0.0);
            if l.proto == Proto::Tcp {
                for c in snap.conns.iter().filter(|c| c.local.port() == l.port) {
                    conns += 1;
                    rate += self.conn_rate.get(&(c.local, c.remote)).copied().unwrap_or(0.0);
                }
            }
            let exposure = self.fw.exposure(l.scope, l.port, l.proto, container.is_some());
            rows.push(Entry {
                proto: l.proto,
                port: l.port,
                addrs: l.addrs,
                pid: l.pid,
                kind,
                title,
                stack,
                service,
                proc_,
                project,
                container,
                conns,
                rate,
                exposure,
            });
        }
        rows.sort_by_key(|e| (e.kind, e.port, e.proto));

        // eventos: portas que abriram/fecharam
        let cur: HashMap<(Proto, u16), String> = rows
            .iter()
            .map(|e| (e.key(), format!("{} {}", e.title, e.stack.clone().unwrap_or_default()).trim().to_string()))
            .collect();
        if !self.first {
            let mut evs = vec![];
            for (k, label) in &cur {
                if !self.known.contains_key(k) {
                    evs.push((true, format!(":{} {}  {}", k.1, k.0.as_str(), label)));
                }
            }
            for (k, label) in &self.known {
                if !cur.contains_key(k) {
                    evs.push((false, format!(":{} {}  {}", k.1, k.0.as_str(), label)));
                }
            }
            for (opened, text) in evs {
                self.push_event(opened, text);
            }
        }
        self.known = cur;
        self.first = false;

        for e in &rows {
            let h = self.hist.entry(e.key()).or_default();
            h.push_back(e.rate as u64);
            while h.len() > 120 {
                h.pop_front();
            }
        }
        self.hist.retain(|k, _| self.known.contains_key(k));

        self.conns = snap.conns;
        self.rows = rows;
        self.rebuild_visible();
        self.schedule_probes();
    }

    fn push_event(&mut self, opened: bool, text: String) {
        self.events.push_front(LogEvent { time: chrono::Local::now().format("%H:%M:%S").to_string(), opened, text });
        self.events.truncate(100);
    }

    fn probe_worthy(e: &Entry) -> bool {
        e.proto == Proto::Tcp && !matches!(e.kind, Kind::Database | Kind::System) && !NON_HTTP_PORTS.contains(&e.port)
    }

    fn schedule_probes(&mut self) {
        let ports: HashSet<u16> = self.rows.iter().map(|e| e.port).collect();
        self.probed.retain(|p| ports.contains(p));
        for e in self.rows.iter().filter(|e| Self::probe_worthy(e)) {
            if self.probed.insert(e.port) {
                self.prober.request(e.port);
            }
        }
        if let Some(e) = self.selected() {
            if Self::probe_worthy(e) {
                let stale = self.prober.get(e.port).is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(10));
                if stale {
                    self.prober.request(e.port);
                }
            }
        }
    }

    // ─── Seleção e filtros ──────────────────────────────────────────────────
    pub fn rebuild_visible(&mut self) {
        let f = self.filter.to_lowercase();
        self.visible = (0..self.rows.len())
            .filter(|&i| {
                let e = &self.rows[i];
                let in_view = match self.view {
                    View::All => true,
                    View::Dev => matches!(e.kind, Kind::Dev | Kind::Tool),
                    View::Db => e.kind == Kind::Database,
                    View::Docker => e.container.is_some(),
                    View::Exposed => e.exposure.is_exposed(),
                };
                in_view
                    && (f.is_empty() || {
                        let hay = format!(
                            "{} {} {} {} {}",
                            e.port,
                            e.title,
                            e.stack.as_deref().unwrap_or(""),
                            e.proc_.as_ref().map(|p| p.cmdline.as_str()).unwrap_or(""),
                            e.service
                        )
                        .to_lowercase();
                        hay.contains(&f)
                    })
            })
            .collect();
        let idx = self
            .sel_key
            .and_then(|k| {
                self.visible
                    .iter()
                    .position(|&i| self.rows[i].key() == k || (self.rows[i].port == k.1 && k.0 == Proto::Tcp))
            })
            .or_else(|| (!self.visible.is_empty()).then(|| self.sel.unwrap_or(0).min(self.visible.len() - 1)));
        self.sel = idx;
        self.sel_key = idx.map(|i| self.rows[self.visible[i]].key());
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.sel.and_then(|i| self.visible.get(i)).map(|&i| &self.rows[i])
    }

    pub fn move_sel(&mut self, delta: i64) {
        if self.visible.is_empty() {
            return;
        }
        let cur = self.sel.unwrap_or(0) as i64;
        let n = (cur + delta).clamp(0, self.visible.len() as i64 - 1) as usize;
        self.select(n);
    }

    /// Seleciona a linha `n` da lista visível (clique do mouse, setas).
    pub fn select(&mut self, n: usize) {
        if n >= self.visible.len() {
            return;
        }
        self.sel = Some(n);
        self.sel_key = Some(self.rows[self.visible[n]].key());
        self.schedule_probes();
    }

    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let dev = self.rows.iter().filter(|e| matches!(e.kind, Kind::Dev | Kind::Tool)).count();
        let db = self.rows.iter().filter(|e| e.kind == Kind::Database).count();
        let dk = self.rows.iter().filter(|e| e.container.is_some()).count();
        let ex = self.rows.iter().filter(|e| e.exposure.is_exposed()).count();
        (dev, db, dk, ex)
    }

    pub fn status(&mut self, msg: impl Into<String>, tone: Tone) {
        self.status = Some((msg.into(), Instant::now(), tone));
    }

    // ─── Ações ──────────────────────────────────────────────────────────────
    pub fn project_dir(&self) -> Option<PathBuf> {
        let e = self.selected()?;
        e.project.as_ref().map(|p| p.root.clone()).or_else(|| e.proc_.as_ref()?.cwd.clone())
    }

    pub fn open_browser(&mut self) {
        let Some(url) = self.selected().map(|e| e.url()) else { return };
        if self.is_root {
            return self.status("não abro o navegador como root", Tone::Warn);
        }
        if spawn_detached("xdg-open", &[&url]) {
            self.status(format!("abrindo {url}"), Tone::Ok);
        }
    }

    pub fn copy_url(&mut self) {
        let Some(url) = self.selected().map(|e| e.url()) else { return };
        let (cmd, args): (&str, &[&str]) =
            if in_path("wl-copy") { ("wl-copy", &[]) } else { ("xclip", &["-selection", "clipboard"]) };
        let ok = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut c| {
                c.stdin.take().unwrap().write_all(url.as_bytes())?;
                c.wait()
            })
            .is_ok();
        if ok {
            self.status(format!("copiado: {url}"), Tone::Ok);
        } else {
            self.status("instale o wl-clipboard para copiar (sudo apt install wl-clipboard)", Tone::Warn);
        }
    }

    pub fn open_editor(&mut self) {
        if self.demo {
            return self.status("modo demonstração: ações desativadas", Tone::Info);
        }
        let Some(dir) = self.project_dir() else {
            return self.status("não sei a pasta deste processo", Tone::Warn);
        };
        let d = dir.to_string_lossy().to_string();
        let gui =
            ["codium", "cursor", "code", "zed", "subl", "kate", "gnome-text-editor"].into_iter().find(|c| in_path(c));
        let (ok, name) = match gui {
            Some(ed) => (spawn_detached(ed, &[&d]), ed.to_string()),
            None => {
                let tui = ["nvim", "vim", "nano"].into_iter().find(|c| in_path(c)).unwrap_or("vi");
                (open_in_terminal(Some(&dir), "editor", &[tui, "."]), tui.to_string())
            }
        };
        if ok {
            self.status(format!("abrindo {d} no {name}"), Tone::Ok);
        } else {
            self.status("nenhum editor encontrado", Tone::Warn);
        }
    }

    pub fn open_terminal(&mut self) {
        if self.demo {
            return self.status("modo demonstração: ações desativadas", Tone::Info);
        }
        let Some(dir) = self.project_dir() else {
            return self.status("não sei a pasta deste processo", Tone::Warn);
        };
        let d = dir.to_string_lossy().to_string();
        if open_in_terminal(Some(&dir), &d, &[]) {
            self.status(format!("terminal em {d}"), Tone::Ok);
        } else {
            self.status("nenhum terminal encontrado (defina a variável TERMINAL)", Tone::Warn);
        }
    }

    pub fn open_logs(&mut self) {
        if self.demo {
            return self.status("modo demonstração: ações desativadas", Tone::Info);
        }
        let Some(name) = self.selected().and_then(|e| e.container.as_ref()).map(|c| c.name.clone()) else {
            return self.status("logs ao vivo só para containers docker", Tone::Warn);
        };
        if open_in_terminal(None, &format!("logs · {name}"), &["docker", "logs", "-f", "--tail", "300", &name]) {
            self.status(format!("logs de {name}"), Tone::Ok);
        } else {
            self.status("nenhum terminal encontrado (defina a variável TERMINAL)", Tone::Warn);
        }
    }

    pub fn ask_kill(&mut self) {
        let Some(e) = self.selected() else { return };
        let (label, target) = if let Some(c) = &e.container {
            (
                format!("parar o container {} (:{})", c.name, e.port),
                KillTarget::Container { id: c.id.clone(), name: c.name.clone() },
            )
        } else if let Some(pid) = e.pid {
            let name = e.proc_.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            let proj = e.project.as_ref().map(|p| format!(" · {}", p.name)).unwrap_or_default();
            (format!("encerrar {name} (PID {pid}{proj}) na :{}", e.port), KillTarget::Pid(pid))
        } else {
            return self.status("processo de root — abra o PortKyu como administrador para encerrar", Tone::Warn);
        };
        self.modal = Modal::Confirm { label, target };
    }

    pub fn kill(&mut self, target: KillTarget, label: &str) {
        if self.demo {
            return self.status(format!("modo demonstração: não encerrei nada ({label})"), Tone::Info);
        }
        match target {
            KillTarget::Pid(pid) => {
                if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
                    return self.status(format!("sem permissão para {label}"), Tone::Err);
                }
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(2500));
                    if unsafe { libc::kill(pid, 0) } == 0 {
                        unsafe { libc::kill(pid, libc::SIGKILL) };
                    }
                });
                self.status(format!("SIGTERM enviado: {label}"), Tone::Ok);
            }
            KillTarget::Container { id, name } => {
                thread::spawn(move || {
                    docker::stop(&id);
                });
                self.status(format!("parando container {name}…"), Tone::Ok);
            }
        }
    }

    pub fn probe_of(&self, e: &Entry) -> Option<Probe> {
        self.prober.get(e.port).map(|(_, p)| p)
    }
}

// ─── Modo demonstração ──────────────────────────────────────────────────────
impl App {
    fn refresh_demo(&mut self) {
        use crate::docker::DPort;
        use std::net::{SocketAddr, SocketAddrV4};
        self.demo_tick += 1;
        let t = self.demo_tick as f64;
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/dev".into()));
        self.lan_ip = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 42)));
        self.fw = Firewall { enabled: true, drop_by_default: true, rules: Some(vec![]) };
        self.last_refresh = Instant::now();

        let proc_ = |pid: i32, name: &str, cmd: &str, dir: &str, rss_mb: u64, up: u64| ProcInfo {
            pid,
            name: name.into(),
            cmdline: cmd.into(),
            cwd: Some(home.join(dir)),
            user: "dev".into(),
            rss: rss_mb * 1024 * 1024,
            uptime_secs: up + self.demo_tick,
            cpu_ticks: 0,
        };
        let project = |dir: &str, name: &str, stack: Option<&str>, lang: &'static str, branch: &str| Project {
            root: home.join(dir),
            name: name.into(),
            stack: stack.map(str::to_string),
            lang,
            branch: Some(branch.into()),
        };
        let container = |name: &str, image: &str, ip: &str, port: u16| Container {
            id: format!("demo-{name}"),
            name: name.into(),
            image: image.into(),
            status: "Up 2 hours (healthy)".into(),
            ports: vec![DPort { ip: ip.into(), public: port, private: port, proto: "tcp".into() }],
        };
        let wave = |base: f64, amp: f64, speed: f64, phase: f64| {
            (base + amp * ((t * speed + phase).sin() * 0.6 + (t * speed * 2.7 + phase).cos() * 0.4)).max(0.0)
        };
        let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let any = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

        // (porta, endereço, pid, processo, projeto, container, conexões, taxa)
        type Spec = (u16, IpAddr, Option<ProcInfo>, Option<Project>, Option<Container>, usize, f64);
        let specs: Vec<Spec> = vec![
            (
                3000,
                any,
                Some(proc_(4211, "next-server", "next-server (v15.1.0)", "dev/acme-web", 612, 5400)),
                Some(project("dev/acme-web", "acme-web", Some("Next.js"), "TypeScript", "feat/checkout")),
                None,
                3,
                wave(42_000.0, 38_000.0, 0.35, 0.0),
            ),
            (
                3001,
                lo,
                Some(proc_(4290, "node", "node --watch dist/main.js", "dev/acme-api", 248, 5300)),
                Some(project("dev/acme-api", "acme-api", Some("NestJS"), "TypeScript", "main")),
                None,
                2,
                wave(9_000.0, 8_000.0, 0.5, 1.3),
            ),
            (
                5173,
                lo,
                Some(proc_(4402, "node", "node node_modules/.bin/vite", "dev/acme-admin", 180, 1900)),
                Some(project("dev/acme-admin", "acme-admin", Some("Vite"), "TypeScript", "fix/table-sort")),
                None,
                1,
                wave(1_200.0, 1_000.0, 0.8, 2.0),
            ),
            (
                8000,
                lo,
                Some(proc_(4517, "uvicorn", "uvicorn app.main:app --reload", "dev/billing", 96, 800)),
                Some(project("dev/billing", "billing", Some("FastAPI"), "Python", "main")),
                None,
                0,
                0.0,
            ),
            (
                9229,
                lo,
                Some(proc_(4211, "next-server", "next-server (v15.1.0)", "dev/acme-web", 612, 5400)),
                None,
                None,
                0,
                0.0,
            ),
            (
                5555,
                lo,
                Some(proc_(4690, "node", "prisma studio", "dev/acme-api", 120, 300)),
                Some(project("dev/acme-api", "acme-api", Some("NestJS"), "TypeScript", "main")),
                None,
                1,
                wave(300.0, 280.0, 0.9, 0.5),
            ),
            (
                5432,
                lo,
                None,
                None,
                Some(container("acme-postgres", "postgres:16-alpine", "127.0.0.1", 5432)),
                4,
                wave(3_500.0, 3_000.0, 0.5, 1.3),
            ),
            (
                6379,
                any,
                None,
                None,
                Some(container("acme-redis", "redis:7-alpine", "0.0.0.0", 6379)),
                1,
                wave(400.0, 350.0, 0.6, 0.2),
            ),
            (
                7700,
                lo,
                None,
                None,
                Some(container("acme-search", "getmeili/meilisearch:v1.12", "127.0.0.1", 7700)),
                0,
                0.0,
            ),
            (53, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 53)), None, None, None, 0, 0.0),
            (631, lo, None, None, None, 0, 0.0),
        ];

        let mut rows = vec![];
        for (port, addr, proc_, project, container, conns, rate) in specs {
            let scope = collect::scope_of(&addr);
            let kind = classify(
                port,
                proc_.as_ref().map(|p| p.name.as_str()),
                container.as_ref().map(|c| c.image.as_str()),
                project.is_some() && port != 9229 && port != 5555,
            );
            let service = describe(port);
            let title = container
                .as_ref()
                .map(|c| c.name.clone())
                .or_else(|| (port != 9229 && port != 5555).then(|| project.as_ref().map(|p| p.name.clone())).flatten())
                .or_else(|| proc_.as_ref().map(|p| p.name.clone()))
                .unwrap_or_else(|| "(root)".into());
            let stack = container
                .as_ref()
                .map(|c| docker::short_image(&c.image))
                .or_else(|| {
                    (port != 9229 && port != 5555)
                        .then(|| project.as_ref().map(|p| p.stack.clone().unwrap_or_else(|| p.lang.to_string())))
                        .flatten()
                })
                .or_else(|| (!service.is_empty()).then(|| service.to_string()));
            let exposure = self.fw.exposure(scope, port, Proto::Tcp, container.is_some());
            rows.push(Entry {
                proto: Proto::Tcp,
                port,
                addrs: vec![addr],
                pid: proc_.as_ref().map(|p| p.pid),
                kind,
                title,
                stack,
                service,
                proc_,
                project,
                container,
                conns,
                rate,
                exposure,
            });
        }
        rows.sort_by_key(|e| (e.kind, e.port, e.proto));

        // conexões fictícias para "clientes" e "fala com"
        let sa = |ip: [u8; 4], port: u16| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), port));
        self.conns.clear();
        self.conn_stats.clear();
        self.conn_rate.clear();
        let add = |app: &mut App, local: SocketAddr, remote: SocketAddr, pid: Option<i32>, rx: u64, tx: u64, r: f64| {
            app.conns.push(Conn { local, remote, pid });
            app.conn_stats.insert((local, remote), ConnStat { rx, tx, rtt_ms: Some(0.05) });
            app.conn_rate.insert((local, remote), r);
        };
        let k = self.demo_tick;
        add(
            self,
            sa([127, 0, 0, 1], 3000),
            sa([127, 0, 0, 1], 51734),
            None,
            48_000 + k * 900,
            2_400_000 + k * 40_000,
            wave(30_000.0, 25_000.0, 0.35, 0.0),
        );
        add(
            self,
            sa([127, 0, 0, 1], 3000),
            sa([127, 0, 0, 1], 51740),
            None,
            12_000 + k * 300,
            640_000 + k * 9_000,
            wave(8_000.0, 7_000.0, 0.35, 1.0),
        );
        add(self, sa([192, 168, 0, 17], 3000), sa([192, 168, 0, 23], 60412), None, 3_100, 210_000, 0.0);
        add(
            self,
            sa([127, 0, 0, 1], 3001),
            sa([127, 0, 0, 1], 51802),
            Some(4211),
            9_000 + k * 200,
            88_000 + k * 3_000,
            wave(6_000.0, 5_000.0, 0.5, 1.3),
        );
        add(self, sa([127, 0, 0, 1], 3001), sa([127, 0, 0, 1], 51806), Some(4211), 4_000, 31_000, 0.0);
        for (i, port) in [5432u16, 5432, 5432, 6379].into_iter().enumerate() {
            add(self, sa([127, 0, 0, 1], 41000 + i as u16), sa([127, 0, 0, 1], port), Some(4290), 120_000, 40_000, 0.0);
        }
        add(self, sa([192, 168, 0, 17], 41900), sa([3, 18, 12, 63], 443), Some(4290), 18_000, 6_000, 0.0);
        add(self, sa([127, 0, 0, 1], 41950), sa([127, 0, 0, 1], 3001), Some(4211), 31_000, 4_000, 0.0);

        for e in &rows {
            let h = self.hist.entry(e.key()).or_default();
            if h.is_empty() {
                // pré-preenche o gráfico para a captura já sair bonita
                for i in 0..90 {
                    let tt = t - 90.0 + i as f64;
                    let v = if e.rate > 0.0 { (e.rate * (0.6 + 0.4 * (tt * 0.31).sin().abs())) as u64 } else { 0 };
                    h.push_back(v);
                }
            }
            h.push_back(e.rate as u64);
            while h.len() > 120 {
                h.pop_front();
            }
        }

        if self.first {
            for (opened, text) in [
                (false, ":4000 tcp  acme-web Next.js"),
                (true, ":3000 tcp  acme-web Next.js"),
                (true, ":5173 tcp  acme-admin Vite"),
                (true, ":8000 tcp  billing FastAPI"),
            ] {
                self.push_event(opened, text.into());
            }
            for (port, p) in [
                (
                    3000u16,
                    Probe::Http {
                        status: 200,
                        reason: "OK".into(),
                        ms: 38,
                        ctype: Some("text/html".into()),
                        server: Some("Next.js".into()),
                        title: Some("Acme — Checkout".into()),
                    },
                ),
                (
                    3001,
                    Probe::Http {
                        status: 404,
                        reason: "Not Found".into(),
                        ms: 4,
                        ctype: Some("application/json".into()),
                        server: Some("Express".into()),
                        title: None,
                    },
                ),
                (
                    5173,
                    Probe::Http {
                        status: 200,
                        reason: "OK".into(),
                        ms: 12,
                        ctype: Some("text/html".into()),
                        server: None,
                        title: Some("Acme Admin".into()),
                    },
                ),
                (
                    8000,
                    Probe::Http {
                        status: 200,
                        reason: "OK".into(),
                        ms: 9,
                        ctype: Some("application/json".into()),
                        server: Some("uvicorn".into()),
                        title: None,
                    },
                ),
                (
                    9229,
                    Probe::Http {
                        status: 400,
                        reason: "Bad Request".into(),
                        ms: 2,
                        ctype: Some("text/plain".into()),
                        server: None,
                        title: None,
                    },
                ),
                (
                    5555,
                    Probe::Http {
                        status: 200,
                        reason: "OK".into(),
                        ms: 21,
                        ctype: Some("text/html".into()),
                        server: None,
                        title: Some("Prisma Studio".into()),
                    },
                ),
            ] {
                self.prober.set(port, p);
            }
        }
        self.first = false;
        self.known = rows.iter().map(|e| (e.key(), e.title.clone())).collect();
        self.rows = rows;
        self.rebuild_visible();
    }
}
