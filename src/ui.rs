//! Desenho da interface.
use crate::app::{App, Entry, Kind, Modal, Tone, VIEWS};
use crate::collect::Proto;
use crate::firewall::Exposure;
use crate::probe::Probe;
use crate::theme::*;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row, Sparkline, Table, TableState, Wrap,
};
use ratatui::Frame;
use std::collections::HashMap;
use std::time::Duration;

fn fg(c: Color) -> Style {
    Style::new().fg(c)
}
fn bold(c: Color) -> Style {
    Style::new().fg(c).add_modifier(Modifier::BOLD)
}

fn kind_color(k: Kind) -> Color {
    match k {
        Kind::Dev => GREEN,
        Kind::Database => BLUE,
        Kind::Tool => PEACH,
        Kind::Other => LAVENDER,
        Kind::System => OVERLAY1,
    }
}

fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Ok => GREEN,
        Tone::Info => SAPPHIRE,
        Tone::Warn => YELLOW,
        Tone::Err => RED,
        Tone::Muted => OVERLAY1,
    }
}

pub fn human_bytes(b: f64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b, 0);
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn rate(r: f64) -> String {
    if r < 1.0 {
        "·".into()
    } else {
        format!("{}/s", human_bytes(r))
    }
}

fn duration(s: u64) -> String {
    let (d, h, m) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {}s", s % 60)
    } else {
        format!("{s}s")
    }
}

/// Corta pela esquerda mantendo o fim (o que importa num caminho): "…/projetos/meu-site"
fn tail(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 2 {
        s.to_string()
    } else {
        format!("…{}", s.chars().skip(n - max + 1).collect::<String>())
    }
}

/// Corta pela direita: comandos longos
fn head(s: &str, max: usize) -> String {
    if s.chars().count() <= max || max < 2 {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn home(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().to_string();
    match std::env::var("HOME") {
        Ok(h) if s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}

pub fn exposure_badge(x: Exposure) -> (&'static str, &'static str, Color) {
    match x {
        Exposure::Local => (I_LOCK, "local", GREEN),
        Exposure::Blocked => (I_SHIELD, "bloqueada", TEAL),
        Exposure::BlockedAssumed => (I_SHIELD, "bloqueada*", TEAL),
        Exposure::Allowed => (I_GLOBE, "liberada", YELLOW),
        Exposure::NoFirewall => (I_WARN, "EXPOSTA", RED),
        Exposure::Docker => (I_WARN, "EXPOSTA", RED),
    }
}

fn exposure_text(x: Exposure) -> &'static str {
    match x {
        Exposure::Local => "só esta máquina acessa (127.0.0.1)",
        Exposure::Blocked => "escuta em todas as interfaces, mas o ufw bloqueia acesso externo",
        Exposure::BlockedAssumed => "na rede, mas o ufw bloqueia por padrão (regras: só como root)",
        Exposure::Allowed => "acessível pela rede — liberada no ufw",
        Exposure::NoFirewall => "acessível pela rede e o firewall está desligado",
        Exposure::Docker => "publicado na rede e o Docker ignora o ufw — use 127.0.0.1:p:p",
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let [header, tabs, main, events, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(12),
        Constraint::Length(7),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, header, app);
    draw_tabs(f, tabs, app);
    let [left, right] = if main.width >= 130 {
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(main)
    } else {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(main)
    };
    draw_table(f, left, app);
    draw_details(f, right, app);
    draw_events(f, events, app);
    draw_footer(f, footer, app);

    match &app.modal {
        Modal::Help => draw_help(f, area),
        Modal::Confirm { label, .. } => draw_confirm(f, area, label),
        Modal::None => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let (dev, db, dk, ex) = app.counts();
    let mut spans = vec![
        Span::styled(format!(" {I_APP}  portkyu "), Style::new().fg(CRUST).bg(MAUVE).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
    ];
    let mut chip = |icon: &str, n: usize, label: &str, c: Color| {
        spans.push(Span::styled(format!("{icon} {n}"), bold(c)));
        spans.push(Span::styled(format!(" {label}   "), fg(OVERLAY1)));
    };
    chip(I_DEV, dev, "dev", GREEN);
    chip(I_DB, db, "bancos", BLUE);
    chip(I_DOCKER, dk, "docker", SAPPHIRE);
    if ex > 0 {
        chip(I_WARN, ex, "expostas", RED);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);

    let mut right = vec![];
    if app.is_root {
        right.push(Span::styled(" root ", Style::new().fg(CRUST).bg(PEACH).add_modifier(Modifier::BOLD)));
        right.push(Span::raw("  "));
    }
    right.push(if app.fw.enabled {
        Span::styled(format!("{I_SHIELD} ufw ativo"), fg(GREEN))
    } else {
        Span::styled(format!("{I_WARN} ufw desligado"), bold(RED))
    });
    if let Some(ip) = app.lan_ip {
        right.push(Span::styled(format!("   {I_GLOBE} {ip}"), fg(OVERLAY1)));
    }
    right.push(Span::styled(format!("   {} ", chrono::Local::now().format("%H:%M:%S")), fg(SUBTEXT0)));
    f.render_widget(Paragraph::new(Line::from(right).right_aligned()), area);
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let area = Rect { y: area.y + 1, height: 1, ..area };
    let mut spans = vec![Span::raw(" ")];
    for (i, (v, name)) in VIEWS.iter().enumerate() {
        if *v == app.view {
            spans.push(Span::styled(format!(" {} {name} ", i + 1), Style::new().fg(CRUST).bg(LAVENDER).add_modifier(Modifier::BOLD)));
        } else {
            spans.push(Span::styled(format!(" {}", i + 1), fg(OVERLAY0)));
            spans.push(Span::styled(format!(" {name} "), fg(SUBTEXT0)));
        }
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);

    let mut right = vec![];
    if app.filtering || !app.filter.is_empty() {
        right.push(Span::styled(" / ", bold(YELLOW)));
        right.push(Span::styled(app.filter.clone(), bold(TEXT)));
        right.push(Span::styled(if app.filtering { "▏" } else { " " }, fg(YELLOW)));
        right.push(Span::styled("  esc limpa ", fg(OVERLAY0)));
    }
    if app.show_udp {
        right.push(Span::styled(" TCP+UDP ", fg(PINK)));
    }
    f.render_widget(Paragraph::new(Line::from(right).right_aligned()), area);
}

fn panel<'a>(title: Line<'a>, focus: bool) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(if focus { SURFACE2 } else { SURFACE1 }))
        .title(title)
}

fn draw_table(f: &mut Frame, area: Rect, app: &mut App) {
    let title = Line::from(vec![
        Span::styled(" Portas abertas ", bold(TEXT)),
        Span::styled(format!("{} ", app.visible.len()), fg(OVERLAY1)),
    ]);
    let block = panel(title, true);
    if app.visible.is_empty() {
        let msg = if app.filter.is_empty() { "nenhuma porta nesta visão" } else { "nada encontrado para o filtro" };
        let p = Paragraph::new(vec![Line::raw(""), Line::styled(msg, fg(OVERLAY0)).centered()]).block(block);
        f.render_widget(p, area);
        return;
    }
    let header = Row::new(["", "PORTA", "SERVIÇO", "STACK", "CONEX", "TRÁFEGO", "ACESSO"])
        .style(bold(OVERLAY1))
        .bottom_margin(1);
    let rows: Vec<Row> = app
        .visible
        .iter()
        .map(|&i| {
            let e = &app.rows[i];
            let (icon, short, col) = exposure_badge(e.exposure);
            let kind_icon = if e.container.is_some() { I_DOCKER } else { e.kind.icon() };
            let kind_col = if e.container.is_some() && e.kind != Kind::Database { SAPPHIRE } else { kind_color(e.kind) };
            let port_style = if e.proto == Proto::Udp { bold(PINK) } else { bold(PEACH) };
            let title_style = if e.pid.is_none() && e.container.is_none() { fg(OVERLAY0) } else { fg(TEXT) };
            Row::new(vec![
                Cell::from(Span::styled(kind_icon, fg(kind_col))),
                Cell::from(Line::from(vec![
                    Span::styled(format!(":{}", e.port), port_style),
                    Span::styled(if e.proto == Proto::Udp { "ᵘ" } else { "" }, fg(PINK)),
                ])),
                Cell::from(Span::styled(e.title.clone(), title_style)),
                Cell::from(Span::styled(e.stack.clone().unwrap_or_default(), fg(LAVENDER))),
                Cell::from(Span::styled(
                    if e.conns > 0 { e.conns.to_string() } else { "·".into() },
                    if e.conns > 0 { bold(YELLOW) } else { fg(OVERLAY0) },
                )),
                Cell::from(Span::styled(rate(e.rate), if e.rate >= 1.0 { fg(TEAL) } else { fg(OVERLAY0) })),
                Cell::from(Line::from(vec![Span::styled(format!("{icon} "), fg(col)), Span::styled(short, fg(col))])),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(7),
            Constraint::Fill(3),
            Constraint::Fill(2),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(block.padding(Padding::horizontal(1)))
    .column_spacing(1)
    .row_highlight_style(Style::new().bg(SURFACE0).add_modifier(Modifier::BOLD))
    .highlight_symbol(Span::styled("▌", fg(MAUVE)));
    let mut st = TableState::default().with_selected(app.sel);
    f.render_stateful_widget(table, area, &mut st);
}

fn section(title: &str, icon: &str) -> Line<'static> {
    Line::from(vec![Span::styled(format!("{icon} "), fg(MAUVE)), Span::styled(title.to_string(), bold(MAUVE))])
}

fn detail_lines(app: &App, e: &Entry, width: usize) -> Vec<Line<'static>> {
    let w = width.saturating_sub(2);
    let mut l: Vec<Line> = vec![];
    let dim = fg(OVERLAY1);
    let sep = || Span::styled(" · ", fg(SURFACE2));

    // Processo
    if let Some(p) = &e.proc_ {
        l.push(section("PROCESSO", I_CHIP));
        let cpu = app.cpu.get(&p.pid).copied().unwrap_or(0.0);
        l.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(p.name.clone(), bold(TEXT)),
            Span::styled(format!("  PID {}", p.pid), dim),
            sep(),
            Span::styled(p.user.clone(), fg(SUBTEXT0)),
            sep(),
            Span::styled(format!("CPU {cpu:.1}%"), fg(if cpu > 50.0 { RED } else if cpu > 10.0 { YELLOW } else { SUBTEXT0 })),
            sep(),
            Span::styled(format!("RAM {}", human_bytes(p.rss as f64)), fg(SUBTEXT0)),
            sep(),
            Span::styled(format!("há {}", duration(p.uptime_secs)), fg(SUBTEXT0)),
        ]));
        l.push(Line::from(vec![Span::raw("  "), Span::styled(head(&p.cmdline, w), dim)]));
    } else if e.container.is_none() {
        l.push(section("PROCESSO", I_CHIP));
        l.push(Line::styled("  processo de root — abra como administrador para ver detalhes", fg(OVERLAY0)));
    }

    // Projeto
    if let Some(p) = &e.project {
        l.push(Line::raw(""));
        l.push(section("PROJETO", I_FOLDER));
        let mut spans = vec![Span::raw("  "), Span::styled(p.name.clone(), bold(TEXT)), Span::raw("  ")];
        if let Some(s) = &p.stack {
            spans.push(Span::styled(format!(" {s} "), Style::new().fg(CRUST).bg(LAVENDER)));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(format!(" {} ", p.lang), Style::new().fg(TEXT).bg(SURFACE1)));
        if let Some(b) = &p.branch {
            spans.push(Span::styled(format!("   {I_BRANCH} {b}"), fg(PEACH)));
        }
        l.push(Line::from(spans));
        l.push(Line::from(vec![Span::raw("  "), Span::styled(tail(&home(&p.root), w), dim)]));
    }

    // Container
    if let Some(c) = &e.container {
        l.push(Line::raw(""));
        l.push(section("CONTAINER", I_DOCKER));
        l.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(c.name.clone(), bold(TEXT)),
            Span::raw("  "),
            Span::styled(format!(" {} ", c.image), Style::new().fg(CRUST).bg(SAPPHIRE)),
            Span::styled(format!("   {}", c.status), fg(GREEN)),
        ]));
        let maps: Vec<String> =
            c.ports.iter().filter(|p| p.public != 0).map(|p| format!("{}:{}→{}", p.ip, p.public, p.private)).collect();
        l.push(Line::from(vec![Span::raw("  "), Span::styled(maps.join("   "), dim)]));
    }

    // HTTP
    if e.proto == Proto::Tcp && !matches!(e.kind, Kind::Database | Kind::System) {
        l.push(Line::raw(""));
        l.push(section("HTTP", I_BOLT));
        match app.probe_of(e) {
            None | Some(Probe::Pending) => l.push(Line::styled("  testando…", fg(OVERLAY0))),
            Some(Probe::Closed) => l.push(Line::styled("  não aceitou conexão local", fg(OVERLAY0))),
            Some(Probe::NotHttp) => {
                l.push(Line::styled("  não responde HTTP (banco, websocket ou protocolo próprio)", fg(OVERLAY0)))
            }
            Some(Probe::Http { status, reason, ms, ctype, server, title }) => {
                let c = match status {
                    200..=299 => GREEN,
                    300..=399 => SAPPHIRE,
                    400..=499 => YELLOW,
                    _ => RED,
                };
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(format!(" {status} "), Style::new().fg(CRUST).bg(c).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" {reason}"), fg(c)),
                    sep(),
                    Span::styled(format!("{ms} ms"), fg(if ms > 1000 { YELLOW } else { SUBTEXT0 })),
                ];
                if let Some(t) = ctype {
                    spans.push(sep());
                    spans.push(Span::styled(t, fg(SUBTEXT0)));
                }
                if let Some(s) = server {
                    spans.push(sep());
                    spans.push(Span::styled(s, fg(LAVENDER)));
                }
                l.push(Line::from(spans));
                if let Some(t) = title {
                    l.push(Line::from(vec![Span::raw("  "), Span::styled(format!("“{t}”"), Style::new().fg(TEXT).add_modifier(Modifier::ITALIC))]));
                }
            }
        }
    }

    // Acesso
    l.push(Line::raw(""));
    let (icon, _, col) = exposure_badge(e.exposure);
    l.push(section("ACESSO", I_LINK));
    l.push(Line::from(vec![Span::raw("  "), Span::styled(format!("{icon} "), fg(col)), Span::styled(exposure_text(e.exposure), fg(col))]));
    let addrs: Vec<String> = e.addrs.iter().map(|a| a.to_string()).collect();
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(e.url(), Style::new().fg(BLUE).add_modifier(Modifier::UNDERLINED)),
    ];
    if e.exposure.is_exposed() || matches!(e.exposure, Exposure::Blocked | Exposure::BlockedAssumed) {
        if let Some(ip) = app.lan_ip {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(format!("http://{ip}:{}", e.port), fg(if e.exposure.is_exposed() { YELLOW } else { OVERLAY0 })));
        }
    }
    l.push(Line::from(spans));
    l.push(Line::from(vec![Span::raw("  "), Span::styled(format!("escuta em {}", addrs.join(", ")), dim)]));
    l
}

fn draw_details(f: &mut Frame, area: Rect, app: &App) {
    let Some(e) = app.selected() else {
        f.render_widget(panel(Line::styled(" Detalhes ", bold(TEXT)), false), area);
        return;
    };
    let title = Line::from(vec![
        Span::styled(format!(" :{} ", e.port), bold(PEACH)),
        Span::styled(format!("{} ", e.title), bold(TEXT)),
        Span::styled(if e.service.is_empty() { String::new() } else { format!("· {} ", e.service) }, fg(OVERLAY1)),
    ]);
    let block = panel(title, true).padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let info = detail_lines(app, e, inner.width as usize);
    let info_h = (info.len() as u16 + 1).min(inner.height.saturating_sub(7));
    let [a, b, c] =
        Layout::vertical([Constraint::Length(info_h), Constraint::Length(5), Constraint::Min(0)]).areas(inner);
    f.render_widget(Paragraph::new(info).wrap(Wrap { trim: false }), a);

    // gráfico de tráfego
    let data: Vec<u64> = app.hist.get(&e.key()).map(|h| h.iter().copied().collect()).unwrap_or_default();
    let w = b.width as usize;
    let data: Vec<u64> = data[data.len().saturating_sub(w)..].to_vec();
    let peak = data.iter().copied().max().unwrap_or(0);
    let spark_title = Line::from(vec![
        Span::styled(" TRÁFEGO ", bold(MAUVE)),
        Span::styled(format!("agora {}/s", human_bytes(e.rate)), fg(TEAL)),
        Span::styled(format!("  ·  pico {}/s  ·  {} {} ", human_bytes(peak as f64), e.conns, if e.conns == 1 { "conexão" } else { "conexões" }), fg(OVERLAY1)),
    ]);
    let spark = Sparkline::default()
        .block(Block::new().borders(Borders::TOP).border_style(fg(SURFACE1)).title(spark_title))
        .data(data)
        .style(fg(TEAL));
    f.render_widget(spark, b);

    // clientes e destinos
    let mut l: Vec<Line> = vec![];
    if e.proto == Proto::Tcp {
        let mut clients: Vec<_> = app.conns.iter().filter(|c| c.local.port() == e.port).collect();
        clients.sort_by_key(|c| {
            let s = app.conn_stats.get(&(c.local, c.remote)).copied().unwrap_or_default();
            std::cmp::Reverse(s.rx + s.tx)
        });
        l.push(Line::from(vec![
            Span::styled(format!("{I_USERS} "), fg(MAUVE)),
            Span::styled(format!("CLIENTES ({})", clients.len()), bold(MAUVE)),
        ]));
        if clients.is_empty() {
            l.push(Line::styled("  nenhum cliente conectado agora", fg(OVERLAY0)));
        }
        for c in clients.iter().take(6) {
            let s = app.conn_stats.get(&(c.local, c.remote)).copied().unwrap_or_default();
            let r = app.conn_rate.get(&(c.local, c.remote)).copied().unwrap_or(0.0);
            let local = c.remote.ip().is_loopback();
            l.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<24}", c.remote.to_string()), fg(TEXT)),
                Span::styled(if local { "local " } else { "REDE  " }, if local { fg(OVERLAY1) } else { bold(YELLOW) }),
                Span::styled(format!("↓ {:<9}", human_bytes(s.rx as f64)), fg(SUBTEXT0)),
                Span::styled(format!("↑ {:<9}", human_bytes(s.tx as f64)), fg(SUBTEXT0)),
                Span::styled(if r >= 1.0 { format!("{:<11}", rate(r)) } else { " ".repeat(11) }, fg(TEAL)),
                Span::styled(s.rtt_ms.map(|x| format!("rtt {x:.1}ms")).unwrap_or_default(), fg(OVERLAY0)),
            ]));
        }
        if clients.len() > 6 {
            l.push(Line::styled(format!("  … e mais {}", clients.len() - 6), fg(OVERLAY0)));
        }
    }
    if let Some(pid) = e.pid {
        let mut dest: HashMap<String, (usize, u16)> = HashMap::new();
        for c in app.conns.iter().filter(|c| c.pid == Some(pid) && c.local.port() != e.port) {
            let ent = dest.entry(c.remote.to_string()).or_insert((0, c.remote.port()));
            ent.0 += 1;
        }
        if !dest.is_empty() {
            let mut dest: Vec<_> = dest.into_iter().collect();
            dest.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
            l.push(Line::raw(""));
            l.push(Line::from(vec![Span::styled(format!("{I_ARROW} "), fg(MAUVE)), Span::styled("FALA COM", bold(MAUVE))]));
            for (addr, (n, port)) in dest.iter().take(6) {
                let svc = match port {
                    5432 => "postgres",
                    3306 => "mysql",
                    6379 => "redis",
                    6333 => "qdrant",
                    7700 => "meilisearch",
                    27017 => "mongodb",
                    443 => "https",
                    80 => "http",
                    _ => "",
                };
                l.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<12}", svc), bold(BLUE)),
                    Span::styled(format!("{:<28}", addr), fg(TEXT)),
                    Span::styled(format!("×{n}"), fg(OVERLAY1)),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(l), c);
}

fn draw_events(f: &mut Frame, area: Rect, app: &App) {
    let block = panel(Line::styled(" Atividade ", bold(TEXT)), false).padding(Padding::horizontal(1));
    let h = block.inner(area).height as usize;
    let lines: Vec<Line> = if app.events.is_empty() {
        vec![Line::styled("nenhuma porta abriu ou fechou desde que o monitor começou — suba um `pnpm dev` para ver aparecer aqui", fg(OVERLAY0))]
    } else {
        app.events
            .iter()
            .take(h)
            .map(|ev| {
                let (icon, word, c) = if ev.opened { ("▲", "abriu ", GREEN) } else { ("▼", "fechou", RED) };
                Line::from(vec![
                    Span::styled(format!("{}  ", ev.time), fg(OVERLAY0)),
                    Span::styled(format!("{icon} {word}  "), bold(c)),
                    Span::styled(ev.text.clone(), fg(TEXT)),
                ])
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    if let Some((msg, t, c)) = &app.status {
        if t.elapsed() < Duration::from_secs(4) {
            f.render_widget(Paragraph::new(Line::styled(format!(" {msg}"), bold(tone_color(*c)))), area);
            return;
        }
    }
    let keys: &[(&str, &str)] = &[
        ("↑↓", "navegar"),
        ("o", "abrir"),
        ("c", "copiar url"),
        ("e", "editor"),
        ("t", "terminal"),
        ("l", "logs"),
        ("x", "encerrar"),
        ("r", "testar"),
        ("/", "filtrar"),
        ("1-5", "visão"),
        ("u", "udp"),
        ("?", "ajuda"),
        ("q", "sair"),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (k, d) in keys {
        spans.push(Span::styled(*k, bold(MAUVE)));
        spans.push(Span::styled(format!(" {d}  "), fg(OVERLAY1)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [v] = Layout::vertical([Constraint::Length(h)]).flex(Flex::Center).areas(area);
    let [r] = Layout::horizontal([Constraint::Length(w)]).flex(Flex::Center).areas(v);
    r
}

fn draw_confirm(f: &mut Frame, area: Rect, label: &str) {
    let w = (label.chars().count() as u16 + 8).clamp(44, area.width);
    let r = centered(area, w, 7);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(RED))
        .title(Line::styled(format!(" {I_WARN} Confirmar "), bold(RED)))
        .padding(Padding::new(2, 2, 1, 0));
    let text = vec![
        Line::styled(format!("{}?", capitalize(label)), bold(TEXT)),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" s ", Style::new().fg(CRUST).bg(RED).add_modifier(Modifier::BOLD)),
            Span::styled(" sim     ", fg(SUBTEXT0)),
            Span::styled(" qualquer tecla ", Style::new().fg(TEXT).bg(SURFACE1)),
            Span::styled(" cancela", fg(SUBTEXT0)),
        ]),
    ];
    f.render_widget(Paragraph::new(text).block(block), r);
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn draw_help(f: &mut Frame, area: Rect) {
    let r = centered(area, 64, 24);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(MAUVE))
        .title(Line::styled(format!(" {I_APP}  Atalhos "), bold(MAUVE)))
        .padding(Padding::new(2, 2, 1, 0));
    let rows: &[(&str, &str)] = &[
        ("↑ ↓  j k", "navegar  (PgUp/PgDn, g/G início/fim)"),
        ("1 … 5  Tab", "visão: tudo · dev · bancos · docker · expostas"),
        ("/", "filtrar por porta, projeto, stack ou comando"),
        ("o  Enter", "abrir http://localhost:porta no navegador"),
        ("c", "copiar a URL"),
        ("e", "abrir a pasta do projeto no editor (VSCodium)"),
        ("t", "abrir um terminal na pasta do projeto"),
        ("l", "logs ao vivo do container docker"),
        ("x  Del", "encerrar o processo / parar o container"),
        ("r", "testar HTTP de novo"),
        ("u", "mostrar/ocultar portas UDP"),
        ("q  Ctrl+C", "sair"),
    ];
    let mut text: Vec<Line> = rows
        .iter()
        .map(|(k, d)| Line::from(vec![Span::styled(format!("{k:<12}"), bold(PEACH)), Span::styled(*d, fg(TEXT))]))
        .collect();
    text.push(Line::raw(""));
    text.push(Line::styled("ACESSO", bold(MAUVE)));
    for x in [Exposure::Local, Exposure::Blocked, Exposure::Allowed, Exposure::Docker] {
        let (i, s, c) = exposure_badge(x);
        text.push(Line::from(vec![Span::styled(format!("{i} {s:<11}"), bold(c)), Span::styled(exposure_text(x), fg(SUBTEXT0))]));
    }
    text.push(Line::raw(""));
    text.push(Line::styled("qualquer tecla fecha", fg(OVERLAY0)).centered());
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }).block(block), r);
}
