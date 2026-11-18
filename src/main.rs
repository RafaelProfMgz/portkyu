//! portas — monitor de portas para devs.
mod app;
mod collect;
mod docker;
mod firewall;
mod gui;
mod probe;
mod project;
mod theme;
mod traffic;
mod tui;
mod ui;

use app::App;
use std::time::Duration;

const HELP: &str = "portkyu — monitor de portas para devs

uso:
  portkyu            abre a janela (teclado e mouse)
  portkyu 3000       abre já com a porta 3000 selecionada
  portkyu --tui      versão de terminal (útil com sudo ou via ssh)
  portkyu -l         lista uma vez e sai (bom para pipe/grep)
  portkyu -u         inclui portas UDP
  portkyu --admin    abre um terminal com a versão de terminal como root (sudo)
  sudo portkyu --tui mostra processos de root e as regras exatas do ufw";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut udp, mut list, mut tui, mut port, mut snapshot) = (false, false, false, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return;
            }
            "-u" | "--udp" => udp = true,
            "-l" | "--list" => list = true,
            "--tui" | "-t" => tui = true,
            "--admin" => {
                let exe = app::self_exe();
                if !app::open_in_terminal(None, "PortKyu — root", &["sudo", &exe, "--tui"]) {
                    eprintln!("nenhum terminal encontrado; rode: sudo {exe} --tui");
                    std::process::exit(1);
                }
                return;
            }
            "-V" | "--version" => {
                println!("portkyu {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--snapshot" => snapshot = it.next().cloned(),
            p if p.parse::<u16>().is_ok() => port = p.parse().ok(),
            other => {
                eprintln!("argumento desconhecido: {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    let mut app = App::new(udp, port);
    app.refresh();

    if list {
        print_list(&app);
        return;
    }
    if let Some(size) = snapshot {
        render_snapshot(&mut app, &size);
        return;
    }
    let has_display = std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    let result = if tui || !has_display {
        tui::run(&mut app).map_err(|e| e.to_string())
    } else {
        gui::run(app).map_err(|e| e.to_string())
    };
    if let Err(e) = result {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

fn print_list(app: &App) {
    use std::io::Write;
    // escreve com tratamento de erro: `portkyu -l | head` fecha o pipe e não deve gerar panic
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{:<5} {:<6} {:<24} {:<20} {:<6} {:<11} PROJETO", "PROTO", "PORTA", "SERVIÇO", "STACK", "CONEX", "ACESSO");
    for e in &app.rows {
        let (_, short, _) = ui::exposure_badge(e.exposure);
        let dir = e
            .project
            .as_ref()
            .map(|p| p.root.display().to_string())
            .or_else(|| e.proc_.as_ref().and_then(|p| p.cwd.as_ref()).map(|c| c.display().to_string()))
            .unwrap_or_default();
        if writeln!(
            out,
            "{:<5} {:<6} {:<24} {:<20} {:<6} {:<11} {}",
            e.proto.as_str(),
            e.port,
            e.title,
            e.stack.as_deref().unwrap_or(""),
            e.conns,
            short,
            dir
        )
        .is_err()
        {
            return;
        }
    }
}

/// Renderiza a interface de terminal em texto (debug/testes): `portas --snapshot 170x42`
fn render_snapshot(app: &mut App, size: &str) {
    use ratatui::backend::TestBackend;
    let (w, h) = size.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).unwrap_or((170, 42));
    std::thread::sleep(Duration::from_millis(1500));
    app.refresh();
    let mut term = ratatui::Terminal::new(TestBackend::new(w, h)).expect("backend de teste");
    term.draw(|f| ui::draw(f, app)).expect("desenho");
    let buf = term.backend().buffer();
    for y in 0..h {
        let line: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
        println!("{}", line.trim_end());
    }
}
