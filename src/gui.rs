//! Interface gráfica (egui/eframe): janela nativa Wayland, controlada por teclado e mouse.
#![allow(deprecated)] // Id::new: ids estáveis por nome são o que queremos aqui
use crate::app::{self, App, Entry, Kind, Modal, Tone, View, VIEWS};
use crate::collect::Proto;
use crate::firewall::Exposure;
use crate::probe::Probe;
use crate::theme::{
    I_APP, I_ARROW, I_BOLT, I_BRANCH, I_CHIP, I_DB, I_DEV, I_DOCKER, I_FOLDER, I_GLOBE, I_LINK, I_LOCK, I_OTHER,
    I_SHIELD, I_SYS, I_TOOL, I_USERS, I_WARN,
};
use crate::ui::human_bytes;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Id, Key,
    Layout, Margin, Pos2, RichText, Sense, Shape, Stroke, TextStyle, Vec2,
};
use egui_extras::{Column, TableBuilder};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

// ─── Paleta Catppuccin Mocha ────────────────────────────────────────────────
const CRUST: Color32 = Color32::from_rgb(0x11, 0x11, 0x1b);
const MANTLE: Color32 = Color32::from_rgb(0x18, 0x18, 0x25);
const BASE: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x2e);
const SURFACE0: Color32 = Color32::from_rgb(0x31, 0x32, 0x44);
const SURFACE1: Color32 = Color32::from_rgb(0x45, 0x47, 0x5a);
const SURFACE2: Color32 = Color32::from_rgb(0x58, 0x5b, 0x70);
const OVERLAY0: Color32 = Color32::from_rgb(0x6c, 0x70, 0x86);
const OVERLAY1: Color32 = Color32::from_rgb(0x7f, 0x84, 0x9c);
const SUBTEXT0: Color32 = Color32::from_rgb(0xa6, 0xad, 0xc8);
const TEXT: Color32 = Color32::from_rgb(0xcd, 0xd6, 0xf4);
const MAUVE: Color32 = Color32::from_rgb(0xcb, 0xa6, 0xf7);
const BLUE: Color32 = Color32::from_rgb(0x89, 0xb4, 0xfa);
const SAPPHIRE: Color32 = Color32::from_rgb(0x74, 0xc7, 0xec);
const TEAL: Color32 = Color32::from_rgb(0x94, 0xe2, 0xd5);
const GREEN: Color32 = Color32::from_rgb(0xa6, 0xe3, 0xa1);
const YELLOW: Color32 = Color32::from_rgb(0xf9, 0xe2, 0xaf);
const PEACH: Color32 = Color32::from_rgb(0xfa, 0xb3, 0x87);
const RED: Color32 = Color32::from_rgb(0xf3, 0x8b, 0xa8);
const PINK: Color32 = Color32::from_rgb(0xf5, 0xc2, 0xe7);
const LAVENDER: Color32 = Color32::from_rgb(0xb4, 0xbe, 0xfe);

/// Precisa ser igual ao nome do arquivo .desktop para o KDE/GNOME mostrarem o ícone certo.
pub const APP_ID: &str = "portkyu";

const I_SEARCH: &str = "\u{f422}";
const I_COPY: &str = "\u{f0c5}";
const I_CODE: &str = "\u{f121}";
const I_TERM: &str = "\u{f120}";
const I_LIST: &str = "\u{f03a}";
const I_REDO: &str = "\u{f01e}";
const I_STOP: &str = "\u{f28d}";
const I_KEY: &str = "\u{f11c}";
const I_CLOCK: &str = "\u{f017}";

fn tint(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Dev => GREEN,
        Kind::Database => BLUE,
        Kind::Tool => PEACH,
        Kind::Other => LAVENDER,
        Kind::System => OVERLAY1,
    }
}

fn kind_icon(e: &Entry) -> (&'static str, Color32) {
    if e.container.is_some() && e.kind != Kind::Database {
        return (I_DOCKER, SAPPHIRE);
    }
    let i = match e.kind {
        Kind::Dev => I_DEV,
        Kind::Database => I_DB,
        Kind::Tool => I_TOOL,
        Kind::Other => I_OTHER,
        Kind::System => I_SYS,
    };
    (i, kind_color(e.kind))
}

fn tone_color(t: Tone) -> Color32 {
    match t {
        Tone::Ok => GREEN,
        Tone::Info => SAPPHIRE,
        Tone::Warn => YELLOW,
        Tone::Err => RED,
        Tone::Muted => OVERLAY1,
    }
}

fn exposure_badge(x: Exposure) -> (&'static str, &'static str, Color32, &'static str) {
    match x {
        Exposure::Local => (I_LOCK, "local", GREEN, "Só esta máquina acessa (127.0.0.1)."),
        Exposure::Blocked => (I_SHIELD, "bloqueada", TEAL, "Escuta na rede, mas o ufw bloqueia acesso externo."),
        Exposure::BlockedAssumed => {
            (I_SHIELD, "bloqueada", TEAL, "Escuta na rede; o ufw bloqueia por padrão (regras exatas só como root).")
        }
        Exposure::Allowed => (I_GLOBE, "liberada", YELLOW, "Acessível pela rede — liberada no ufw."),
        Exposure::NoFirewall => (I_WARN, "exposta", RED, "Acessível pela rede e o firewall está desligado."),
        Exposure::Docker => (
            I_WARN,
            "exposta",
            RED,
            "Container publicado na rede — o Docker ignora o ufw. Para restringir, use 127.0.0.1:porta:porta no compose.",
        ),
    }
}

fn rate(r: f64) -> String {
    if r < 1.0 {
        "—".into()
    } else {
        format!("{}/s", human_bytes(r))
    }
}

fn duration(s: u64) -> String {
    let (d, h, m) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}min")
    } else if m > 0 {
        format!("{m}min {}s", s % 60)
    } else {
        format!("{s}s")
    }
}

fn home(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().to_string();
    match std::env::var("HOME") {
        Ok(h) if s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}

// ─── Ações (coletadas durante o desenho e aplicadas no fim do frame) ───────
enum Act {
    Select(usize),
    Move(i64),
    Open,
    OpenUrl(String),
    Copy,
    Editor,
    Terminal,
    Logs,
    Probe,
    AskKill,
    ConfirmKill,
    CloseModal,
    View(View),
    ToggleUdp,
    Help,
    FocusSearch,
    ClearSearch,
    Quit,
}

pub struct Gui {
    app: App,
    search_id: Id,
    focus_search: bool,
    scroll_to_sel: bool,
}

// ─── Configuração visual ────────────────────────────────────────────────────
fn font_file(pattern: &str) -> Option<Vec<u8>> {
    let out = Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    if path.is_empty() {
        return None;
    }
    std::fs::read(path).ok()
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    // ícones: fonte de símbolos Nerd Fonts embutida (funciona mesmo sem fontes Nerd instaladas).
    // Fica em primeiro lugar porque só tem glifos da área privada (U+E000–U+F8FF): letras caem na fonte seguinte.
    fonts.font_data.insert(
        "icons".into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf"))),
    );
    let mut add = |name: &str, pattern: &str| -> bool {
        match font_file(pattern) {
            Some(b) => {
                fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(b)));
                true
            }
            None => false,
        }
    };
    let ui = add("ui", "Noto Sans:style=Regular");
    let ui_bold = add("ui-bold", "Noto Sans:style=SemiBold");
    let mono = add("mono", "JetBrainsMono Nerd Font:style=Regular");
    let mono_bold = add("mono-bold", "JetBrainsMono Nerd Font:style=Bold");

    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    if ui {
        prop.insert(0, "ui".into());
    }
    prop.insert(0, "icons".into());
    let monos = fonts.families.entry(FontFamily::Monospace).or_default();
    if mono {
        monos.insert(0, "mono".into());
    }
    monos.insert(0, "icons".into());
    let mut b: Vec<String> = vec!["icons".into()];
    if ui_bold {
        b.push("ui-bold".into());
    }
    b.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(FontFamily::Name("bold".into()), b);
    let mut mb: Vec<String> = vec!["icons".into()];
    if mono_bold {
        mb.push("mono-bold".into());
    }
    mb.extend(fonts.families[&FontFamily::Monospace].iter().cloned());
    fonts.families.insert(FontFamily::Name("mono-bold".into()), mb);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        v.dark_mode = true;
        v.panel_fill = BASE;
        v.window_fill = MANTLE;
        v.extreme_bg_color = CRUST;
        v.faint_bg_color = MANTLE;
        v.code_bg_color = SURFACE0;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = BLUE;
        v.selection.bg_fill = tint(MAUVE, 46);
        v.selection.stroke = Stroke::new(1.0, MAUVE);
        v.window_corner_radius = CornerRadius::same(14);
        v.window_stroke = Stroke::new(1.0, SURFACE1);
        v.menu_corner_radius = CornerRadius::same(10);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, SURFACE0);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(8);
        }
        v.widgets.inactive.bg_fill = SURFACE0;
        v.widgets.inactive.weak_bg_fill = SURFACE0;
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.bg_fill = SURFACE1;
        v.widgets.hovered.weak_bg_fill = SURFACE1;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, SURFACE2);
        v.widgets.hovered.fg_stroke = Stroke::new(1.5, TEXT);
        v.widgets.active.bg_fill = SURFACE2;
        v.widgets.active.weak_bg_fill = SURFACE2;
        v.widgets.active.bg_stroke = Stroke::new(1.0, MAUVE);
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(11.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Heading, bold(20.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.5));
    });
}

pub fn run(mut app: App) -> eframe::Result<()> {
    // depuração visual: PORTAS_MODAL=help|confirm abre já com o diálogo
    match std::env::var("PORTAS_MODAL").as_deref() {
        Ok("help") => app.modal = Modal::Help,
        Ok("confirm") => app.ask_kill(),
        _ => {}
    }
    let mut vp = egui::ViewportBuilder::default()
        .with_title("PortKyu")
        .with_app_id(APP_ID)
        .with_inner_size([1400.0, 880.0])
        .with_min_inner_size([980.0, 620.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/portkyu-256.png")) {
        vp = vp.with_icon(Arc::new(icon));
    }
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "PortKyu",
        opts,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(Gui { app, search_id: Id::new("busca"), focus_search: false, scroll_to_sel: true }))
        }),
    )
}

// ─── Componentes ────────────────────────────────────────────────────────────
/// Etiqueta arredondada desenhada à mão: tamanho exato do texto, sem esticar na linha.
fn pill(ui: &mut egui::Ui, text: impl Into<String>, color: Color32, filled: bool) -> egui::Response {
    let fg = if filled { CRUST } else { color };
    let galley = ui.painter().layout_no_wrap(text.into(), FontId::proportional(12.5), fg);
    let pad = Vec2::new(9.0, 3.5);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 10, if filled { color } else { tint(color, 34) });
        ui.painter().galley(rect.min + pad, galley, fg);
    }
    resp
}

fn card(ui: &mut egui::Ui, title: &str, icon: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(BASE)
        .stroke(Stroke::new(1.0, SURFACE0))
        .corner_radius(12)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(MAUVE).size(13.0));
                ui.label(RichText::new(title).font(bold(12.0)).color(MAUVE));
            });
            ui.add_space(6.0);
            add(ui);
        });
    ui.add_space(10.0);
}

fn action_button(ui: &mut egui::Ui, icon: &str, label: &str, color: Color32, primary: bool, hint: &str) -> bool {
    let text = RichText::new(format!("{icon}  {label}")).color(if primary { CRUST } else { color }).size(13.5);
    let btn = egui::Button::new(text)
        .fill(if primary { color } else { tint(color, 26) })
        .stroke(Stroke::new(1.0, tint(color, if primary { 255 } else { 70 })))
        .corner_radius(9);
    ui.add(btn).on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn kv(ui: &mut egui::Ui, k: &str, v: RichText) {
    ui.label(RichText::new(k).color(OVERLAY1).size(13.0));
    ui.label(v);
    ui.end_row();
}

fn traffic_chart(ui: &mut egui::Ui, data: &[u64], color: Color32) {
    let h = 96.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 10, CRUST);
    for i in 1..4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        p.line_segment([Pos2::new(rect.left() + 8.0, y), Pos2::new(rect.right() - 8.0, y)], Stroke::new(1.0, tint(SURFACE0, 160)));
    }
    if data.len() < 2 {
        p.text(rect.center(), Align2::CENTER_CENTER, "coletando…", FontId::proportional(12.0), OVERLAY0);
        return;
    }
    let max = data.iter().copied().max().unwrap_or(0).max(1) as f32;
    let slots = 120.0;
    let step = (rect.width() - 16.0) / (slots - 1.0);
    let n = data.len();
    let pts: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.right() - 8.0 - (n - 1 - i) as f32 * step;
            let y = rect.bottom() - 6.0 - (*v as f32 / max) * (h - 18.0);
            Pos2::new(x, y)
        })
        .collect();
    let mut mesh = egui::Mesh::default();
    let base_y = rect.bottom() - 6.0;
    for w in pts.windows(2) {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(w[0], tint(color, 70));
        mesh.colored_vertex(w[1], tint(color, 70));
        mesh.colored_vertex(Pos2::new(w[1].x, base_y), tint(color, 4));
        mesh.colored_vertex(Pos2::new(w[0].x, base_y), tint(color, 4));
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::line(pts, Stroke::new(2.0, color)));
    if data.iter().all(|v| *v == 0) {
        p.text(rect.center(), Align2::CENTER_CENTER, "sem tráfego", FontId::proportional(12.0), OVERLAY0);
    }
}

// ─── App ────────────────────────────────────────────────────────────────────
impl eframe::App for Gui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.app.last_refresh.elapsed() >= Duration::from_millis(1000) {
            self.app.refresh();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(300));
        let mut acts: Vec<Act> = vec![];
        self.keyboard(ui.ctx(), &mut acts);

        egui::Panel::top("topo")
            .frame(Frame::new().fill(MANTLE).inner_margin(Margin { left: 18, right: 18, top: 14, bottom: 8 }))
            .show(ui, |ui| self.top_bar(ui, &mut acts));
        egui::Panel::top("abas")
            .frame(Frame::new().fill(MANTLE).inner_margin(Margin { left: 18, right: 18, top: 4, bottom: 12 }))
            .show(ui, |ui| self.tabs(ui, &mut acts));
        egui::Panel::bottom("status")
            .frame(Frame::new().fill(CRUST).inner_margin(Margin::symmetric(16, 6)))
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("atividade")
            .resizable(true)
            .default_size(150.0)
            .min_size(80.0)
            .frame(Frame::new().fill(MANTLE).inner_margin(Margin::symmetric(18, 10)))
            .show(ui, |ui| self.activity(ui));
        egui::Panel::right("detalhes")
            .resizable(true)
            .default_size(500.0)
            .min_size(400.0)
            .frame(Frame::new().fill(MANTLE).inner_margin(Margin::same(16)))
            .show(ui, |ui| self.details(ui, &mut acts));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BASE).inner_margin(Margin::symmetric(12, 10)))
            .show(ui, |ui| self.table(ui, &mut acts));

        self.modals(ui.ctx(), &mut acts);
        for a in acts {
            self.apply(a, ui.ctx());
        }
    }
}

impl Gui {
    fn apply(&mut self, a: Act, ctx: &egui::Context) {
        let app = &mut self.app;
        match a {
            Act::Select(n) => app.select(n),
            Act::Move(d) => {
                app.move_sel(d);
                self.scroll_to_sel = true;
            }
            Act::Open => app.open_browser(),
            Act::OpenUrl(u) => {
                app::spawn_detached("xdg-open", &[&u]);
                app.status(format!("abrindo {u}"), Tone::Ok);
            }
            Act::Copy => app.copy_url(),
            Act::Editor => app.open_editor(),
            Act::Terminal => app.open_terminal(),
            Act::Logs => app.open_logs(),
            Act::Probe => {
                if let Some(p) = app.selected().map(|e| e.port) {
                    app.prober.request(p);
                    app.status(format!("testando http://localhost:{p} …"), Tone::Info);
                }
            }
            Act::AskKill => app.ask_kill(),
            Act::ConfirmKill => {
                if let Modal::Confirm { label, target } = std::mem::replace(&mut app.modal, Modal::None) {
                    app.kill(target, &label);
                }
            }
            Act::CloseModal => app.modal = Modal::None,
            Act::View(v) => {
                app.view = v;
                app.rebuild_visible();
                self.scroll_to_sel = true;
            }
            Act::ToggleUdp => {
                app.show_udp = !app.show_udp;
                app.refresh();
            }
            Act::Help => app.modal = Modal::Help,
            Act::FocusSearch => self.focus_search = true,
            Act::ClearSearch => {
                app.filter.clear();
                app.rebuild_visible();
                ctx.memory_mut(|m| m.surrender_focus(self.search_id));
            }
            Act::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let typing = ctx.memory(|m| m.has_focus(self.search_id));
        let modal = match self.app.modal {
            Modal::None => 0,
            Modal::Help => 1,
            Modal::Confirm { .. } => 2,
        };
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;
            if ctrl && i.key_pressed(Key::Q) {
                acts.push(Act::Quit);
            }
            if modal == 2 {
                if i.key_pressed(Key::Enter) || i.key_pressed(Key::S) {
                    acts.push(Act::ConfirmKill);
                } else if i.key_pressed(Key::Escape) || i.key_pressed(Key::N) {
                    acts.push(Act::CloseModal);
                }
                return;
            }
            if modal == 1 {
                if i.key_pressed(Key::Escape) || i.key_pressed(Key::F1) || i.key_pressed(Key::Enter) {
                    acts.push(Act::CloseModal);
                }
                return;
            }
            if ctrl && i.key_pressed(Key::F) {
                acts.push(Act::FocusSearch);
            }
            if i.key_pressed(Key::ArrowDown) {
                acts.push(Act::Move(1));
            }
            if i.key_pressed(Key::ArrowUp) {
                acts.push(Act::Move(-1));
            }
            if i.key_pressed(Key::PageDown) {
                acts.push(Act::Move(10));
            }
            if i.key_pressed(Key::PageUp) {
                acts.push(Act::Move(-10));
            }
            if typing {
                if i.key_pressed(Key::Escape) {
                    acts.push(Act::ClearSearch);
                }
                if i.key_pressed(Key::Enter) {
                    acts.push(Act::Open);
                }
                return;
            }
            let k = |key| i.key_pressed(key) && !ctrl;
            if k(Key::J) {
                acts.push(Act::Move(1));
            }
            if k(Key::K) {
                acts.push(Act::Move(-1));
            }
            if k(Key::Home) || k(Key::G) && !i.modifiers.shift {
                acts.push(Act::Move(-100_000));
            }
            if k(Key::End) || (i.modifiers.shift && i.key_pressed(Key::G)) {
                acts.push(Act::Move(100_000));
            }
            if k(Key::Enter) || k(Key::O) {
                acts.push(Act::Open);
            }
            if k(Key::C) {
                acts.push(Act::Copy);
            }
            if k(Key::E) {
                acts.push(Act::Editor);
            }
            if k(Key::T) {
                acts.push(Act::Terminal);
            }
            if k(Key::L) {
                acts.push(Act::Logs);
            }
            if k(Key::R) {
                acts.push(Act::Probe);
            }
            if k(Key::X) || k(Key::Delete) {
                acts.push(Act::AskKill);
            }
            if k(Key::U) {
                acts.push(Act::ToggleUdp);
            }
            if k(Key::Slash) {
                acts.push(Act::FocusSearch);
            }
            if k(Key::F1) || k(Key::Questionmark) {
                acts.push(Act::Help);
            }
            if k(Key::Escape) {
                acts.push(Act::ClearSearch);
            }
            for (n, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].into_iter().enumerate() {
                if k(key) {
                    acts.push(Act::View(VIEWS[n].0));
                }
            }
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let (dev, db, dk, ex) = self.app.counts();
        ui.horizontal(|ui| {
            // logo
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 11, SURFACE0);
            p.rect_stroke(rect, 11, Stroke::new(1.0, tint(MAUVE, 90)), egui::StrokeKind::Inside);
            p.text(rect.center(), Align2::CENTER_CENTER, I_APP, FontId::monospace(18.0), MAUVE);
            p.circle_filled(Pos2::new(rect.right() - 6.0, rect.top() + 6.0), 3.5, GREEN);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new("PortKyu").font(bold(19.0)).color(TEXT));
                ui.label(RichText::new("monitor de portas para dev").size(12.0).color(OVERLAY1));
            });
            ui.add_space(22.0);

            let chips = [
                (I_DEV, dev, "dev", GREEN, View::Dev),
                (I_DB, db, "bancos", BLUE, View::Db),
                (I_DOCKER, dk, "docker", SAPPHIRE, View::Docker),
                (I_WARN, ex, "expostas", if ex > 0 { RED } else { OVERLAY1 }, View::Exposed),
            ];
            for (icon, n, label, color, view) in chips {
                let active = self.app.view == view;
                let btn = egui::Button::new(
                    RichText::new(format!("{icon}  {n}  {label}")).color(if active { CRUST } else { color }).size(13.5),
                )
                .fill(if active { color } else { tint(color, 24) })
                .stroke(Stroke::new(1.0, tint(color, if active { 255 } else { 60 })))
                .corner_radius(15);
                let r = ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("mostrar só {label}"));
                if r.clicked() {
                    acts.push(Act::View(if active { View::All } else { view }));
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(chrono::Local::now().format("%H:%M:%S").to_string()).monospace().color(SUBTEXT0));
                ui.label(RichText::new(I_CLOCK).color(OVERLAY0));
                ui.add_space(10.0);
                if let Some(ip) = self.app.lan_ip {
                    ui.label(RichText::new(ip.to_string()).monospace().color(OVERLAY1)).on_hover_text("IP desta máquina na rede local");
                    ui.label(RichText::new(I_GLOBE).color(OVERLAY0));
                    ui.add_space(10.0);
                }
                if self.app.fw.enabled {
                    pill(ui, format!("{I_SHIELD}  ufw ativo"), GREEN, false).on_hover_text("firewall ligado, bloqueando conexões de entrada");
                } else {
                    pill(ui, format!("{I_WARN}  ufw desligado"), RED, true);
                }
                ui.add_space(14.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.app.filter)
                        .id(self.search_id)
                        .hint_text(RichText::new(format!("{I_SEARCH}   Buscar porta, projeto, stack…   ( / )")).color(OVERLAY0))
                        .desired_width(320.0)
                        .margin(Margin::symmetric(10, 6)),
                );
                if r.changed() {
                    self.app.rebuild_visible();
                    self.scroll_to_sel = true;
                }
                if self.focus_search {
                    r.request_focus();
                    self.focus_search = false;
                }
            });
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let counts = |v: View, app: &App| -> usize {
            app.rows
                .iter()
                .filter(|e| match v {
                    View::All => true,
                    View::Dev => matches!(e.kind, Kind::Dev | Kind::Tool),
                    View::Db => e.kind == Kind::Database,
                    View::Docker => e.container.is_some(),
                    View::Exposed => e.exposure.is_exposed(),
                })
                .count()
        };
        ui.horizontal(|ui| {
            Frame::new().fill(CRUST).corner_radius(11).inner_margin(Margin::same(3)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, (v, name)) in VIEWS.iter().enumerate() {
                        let active = self.app.view == *v;
                        let n = counts(*v, &self.app);
                        let text = RichText::new(format!("{name}  {n}"))
                            .font(if active { bold(13.5) } else { FontId::proportional(13.5) })
                            .color(if active { MAUVE } else { SUBTEXT0 });
                        let btn = egui::Button::new(text)
                            .fill(if active { SURFACE0 } else { Color32::TRANSPARENT })
                            .stroke(Stroke::NONE)
                            .corner_radius(8)
                            .min_size(Vec2::new(88.0, 28.0));
                        if ui.add(btn).on_hover_text(format!("tecla {}", i + 1)).clicked() {
                            acts.push(Act::View(*v));
                        }
                    }
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(format!("{I_KEY}  Atalhos")).size(13.0)).corner_radius(8)).on_hover_text("F1 ou ?").clicked() {
                    acts.push(Act::Help);
                }
                let mut udp = self.app.show_udp;
                if ui.checkbox(&mut udp, RichText::new("incluir UDP").size(13.0).color(SUBTEXT0)).on_hover_text("tecla U").changed() {
                    acts.push(Act::ToggleUdp);
                }
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{} portas · atualiza a cada 1s", self.app.rows.len())).size(12.5).color(OVERLAY0));
            });
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let app = &self.app;
        if app.visible.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(if app.filter.is_empty() { "Nenhuma porta nesta visão" } else { "Nada encontrado para a busca" })
                        .size(16.0)
                        .color(OVERLAY0),
                );
            });
            return;
        }
        let mut tb = TableBuilder::new(ui)
            .id_salt("tabela")
            .striped(false)
            .sense(Sense::click())
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(30.0))
            .column(Column::exact(82.0))
            .column(Column::initial(250.0).at_least(140.0).clip(true))
            .column(Column::initial(150.0).at_least(80.0).clip(true))
            .column(Column::exact(64.0))
            .column(Column::exact(100.0))
            .column(Column::remainder().at_least(120.0));
        if self.scroll_to_sel {
            if let Some(s) = app.sel {
                tb = tb.scroll_to_row(s, None);
            }
            self.scroll_to_sel = false;
        }
        tb.header(30.0, |mut h| {
            for t in ["", "PORTA", "SERVIÇO", "STACK", "CONEX.", "TRÁFEGO", "ACESSO"] {
                h.col(|ui| {
                    ui.label(RichText::new(t).font(bold(11.5)).color(OVERLAY1));
                });
            }
        })
        .body(|mut body| {
            for (ri, &ei) in app.visible.iter().enumerate() {
                let e = &app.rows[ei];
                body.row(48.0, |mut row| {
                    row.set_selected(app.sel == Some(ri));
                    row.col(|ui| {
                        let (i, c) = kind_icon(e);
                        ui.label(RichText::new(i).size(16.0).color(c));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format!(":{}", e.port))
                                .font(FontId::new(15.5, FontFamily::Name("mono-bold".into())))
                                .color(if e.proto == Proto::Udp { PINK } else { PEACH }),
                        );
                        if e.proto == Proto::Udp {
                            ui.label(RichText::new("udp").size(10.5).color(PINK));
                        }
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.add_space(3.0);
                            let dim = e.pid.is_none() && e.container.is_none();
                            ui.label(RichText::new(&e.title).font(bold(14.5)).color(if dim { OVERLAY1 } else { TEXT }));
                            let sub = if let Some(p) = &e.project {
                                home(&p.root)
                            } else if let Some(c) = &e.container {
                                c.status.clone()
                            } else if let Some(p) = &e.proc_ {
                                p.cmdline.clone()
                            } else {
                                e.service.to_string()
                            };
                            ui.label(RichText::new(sub).size(11.5).color(OVERLAY0));
                        });
                    });
                    row.col(|ui| {
                        if let Some(s) = &e.stack {
                            pill(ui, s, LAVENDER, false);
                        }
                    });
                    row.col(|ui| {
                        if e.conns > 0 {
                            ui.label(RichText::new(e.conns.to_string()).font(bold(14.0)).color(YELLOW));
                        } else {
                            ui.label(RichText::new("—").color(SURFACE2));
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(rate(e.rate)).monospace().color(if e.rate >= 1.0 { TEAL } else { SURFACE2 }));
                    });
                    row.col(|ui| {
                        let (icon, short, c, tip) = exposure_badge(e.exposure);
                        pill(ui, format!("{icon}  {short}"), c, false).on_hover_text(tip);
                    });

                    let r = row.response();
                    if r.clicked() {
                        acts.push(Act::Select(ri));
                    }
                    if r.double_clicked() {
                        acts.push(Act::Select(ri));
                        acts.push(Act::Open);
                    }
                    if r.secondary_clicked() {
                        acts.push(Act::Select(ri));
                    }
                    let is_docker = e.container.is_some();
                    let port = e.port;
                    r.context_menu(|ui| {
                        ui.set_min_width(220.0);
                        ui.label(RichText::new(format!(":{port}  {}", e.title)).font(bold(13.0)).color(PEACH));
                        ui.separator();
                        let mut item = |ui: &mut egui::Ui, icon: &str, label: &str, key: &str, act: Act| {
                            let b = egui::Button::new(RichText::new(format!("{icon}   {label}"))).shortcut_text(key);
                            if ui.add(b).clicked() {
                                acts.push(Act::Select(ri));
                                acts.push(act);
                                ui.close();
                            }
                        };
                        item(ui, I_GLOBE, "Abrir no navegador", "Enter", Act::Open);
                        item(ui, I_COPY, "Copiar URL", "C", Act::Copy);
                        item(ui, I_CODE, "Abrir projeto no editor", "E", Act::Editor);
                        item(ui, I_TERM, "Terminal na pasta", "T", Act::Terminal);
                        if is_docker {
                            item(ui, I_LIST, "Logs do container", "L", Act::Logs);
                        }
                        item(ui, I_REDO, "Testar HTTP", "R", Act::Probe);
                        ui.separator();
                        item(ui, I_STOP, if is_docker { "Parar container" } else { "Encerrar processo" }, "Del", Act::AskKill);
                    });
                });
            }
        });
    }

    fn details(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let app = &self.app;
        let Some(e) = app.selected() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Selecione uma porta").size(15.0).color(OVERLAY0));
            });
            return;
        };
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            // cabeçalho
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!(":{}", e.port)).font(FontId::new(30.0, FontFamily::Name("mono-bold".into()))).color(PEACH));
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(RichText::new(&e.title).font(bold(19.0)).color(TEXT));
                    let mut sub = vec![e.proto.as_str().to_uppercase()];
                    if !e.service.is_empty() {
                        sub.push(e.service.to_string());
                    }
                    ui.label(RichText::new(sub.join(" · ")).size(12.5).color(OVERLAY1));
                });
            });
            ui.add_space(10.0);

            // ações
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                if action_button(ui, I_GLOBE, "Abrir", MAUVE, true, "Abrir no navegador (Enter / duplo clique)") {
                    acts.push(Act::Open);
                }
                if action_button(ui, I_COPY, "Copiar URL", BLUE, false, "Copiar http://localhost:porta (C)") {
                    acts.push(Act::Copy);
                }
                if action_button(ui, I_CODE, "Editor", BLUE, false, "Abrir a pasta do projeto no editor (E)") {
                    acts.push(Act::Editor);
                }
                if action_button(ui, I_TERM, "Terminal", BLUE, false, "Abrir um terminal na pasta do projeto (T)") {
                    acts.push(Act::Terminal);
                }
                if e.container.is_some() && action_button(ui, I_LIST, "Logs", SAPPHIRE, false, "Logs ao vivo do container (L)") {
                    acts.push(Act::Logs);
                }
                if action_button(ui, I_REDO, "Testar", TEAL, false, "Testar HTTP de novo (R)") {
                    acts.push(Act::Probe);
                }
                let stop = if e.container.is_some() { "Parar" } else { "Encerrar" };
                if action_button(ui, I_STOP, stop, RED, false, "Encerrar processo / parar container (Del)") {
                    acts.push(Act::AskKill);
                }
            });
            ui.add_space(14.0);

            // processo
            if let Some(p) = &e.proc_ {
                card(ui, "PROCESSO", I_CHIP, |ui| {
                    let cpu = app.cpu.get(&p.pid).copied().unwrap_or(0.0);
                    egui::Grid::new("proc").num_columns(2).spacing([16.0, 7.0]).show(ui, |ui| {
                        kv(ui, "Programa", RichText::new(&p.name).font(bold(14.0)));
                        kv(ui, "PID", RichText::new(p.pid.to_string()).monospace());
                        kv(ui, "Usuário", RichText::new(&p.user));
                        ui.label(RichText::new("CPU").color(OVERLAY1).size(13.0));
                        let c = if cpu > 50.0 { RED } else if cpu > 10.0 { YELLOW } else { GREEN };
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new((cpu / 100.0).clamp(0.0, 1.0) as f32)
                                    .desired_width(160.0)
                                    .desired_height(10.0)
                                    .fill(c),
                            );
                            ui.label(RichText::new(format!("{cpu:.1}%")).color(c));
                        });
                        ui.end_row();
                        kv(ui, "Memória", RichText::new(human_bytes(p.rss as f64)));
                        kv(ui, "Rodando há", RichText::new(duration(p.uptime_secs)));
                    });
                    ui.add_space(6.0);
                    Frame::new().fill(CRUST).corner_radius(8).inner_margin(Margin::symmetric(10, 7)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(&p.cmdline).monospace().size(12.5).color(SUBTEXT0)).wrap());
                    });
                });
            } else if e.container.is_none() {
                card(ui, "PROCESSO", I_CHIP, |ui| {
                    ui.label(RichText::new("Processo de root — para ver os detalhes, abra o PortKyu como administrador (botão direito no ícone).").color(OVERLAY1));
                });
            }

            // projeto
            if let Some(p) = &e.project {
                card(ui, "PROJETO", I_FOLDER, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&p.name).font(bold(16.0)));
                        ui.add_space(4.0);
                        if let Some(s) = &p.stack {
                            pill(ui, s, LAVENDER, true);
                        }
                        pill(ui, p.lang, SUBTEXT0, false);
                        if let Some(b) = &p.branch {
                            pill(ui, format!("{I_BRANCH}  {b}"), PEACH, false).on_hover_text("branch atual do git");
                        }
                    });
                    ui.add_space(4.0);
                    let r = ui
                        .add(egui::Label::new(RichText::new(format!("{I_FOLDER}  {}", home(&p.root))).color(BLUE).size(13.0)).sense(Sense::click()))
                        .on_hover_text("clique para abrir no editor")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        acts.push(Act::Editor);
                    }
                });
            }

            // container
            if let Some(c) = &e.container {
                card(ui, "CONTAINER", I_DOCKER, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&c.name).font(bold(16.0)));
                        pill(ui, &c.image, SAPPHIRE, true);
                    });
                    ui.label(RichText::new(&c.status).color(GREEN));
                    for p in c.ports.iter().filter(|p| p.public != 0) {
                        ui.label(RichText::new(format!("{}:{}  →  {}/{}", p.ip, p.public, p.private, p.proto)).monospace().size(12.5).color(SUBTEXT0));
                    }
                });
            }

            // http
            if e.proto == Proto::Tcp && !matches!(e.kind, Kind::Database | Kind::System) {
                card(ui, "HTTP", I_BOLT, |ui| match app.probe_of(e) {
                    None | Some(Probe::Pending) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("testando…").color(OVERLAY1));
                        });
                    }
                    Some(Probe::Closed) => {
                        ui.label(RichText::new("Não aceitou conexão local.").color(OVERLAY1));
                    }
                    Some(Probe::NotHttp) => {
                        ui.label(RichText::new("Não responde HTTP — banco, websocket ou protocolo próprio.").color(OVERLAY1));
                    }
                    Some(Probe::Http { status, reason, ms, ctype, server, title }) => {
                        let c = match status {
                            200..=299 => GREEN,
                            300..=399 => SAPPHIRE,
                            400..=499 => YELLOW,
                            _ => RED,
                        };
                        ui.horizontal_wrapped(|ui| {
                            pill(ui, format!("{status}"), c, true);
                            ui.label(RichText::new(reason).color(c));
                            ui.label(RichText::new(format!("{ms} ms")).color(if ms > 1000 { YELLOW } else { SUBTEXT0 }));
                            if let Some(t) = ctype {
                                ui.label(RichText::new(t).color(OVERLAY1).size(12.5));
                            }
                        });
                        if let Some(s) = server {
                            ui.label(RichText::new(s).color(LAVENDER).size(13.0));
                        }
                        if let Some(t) = title {
                            ui.add_space(2.0);
                            ui.label(RichText::new(format!("“{t}”")).italics().size(14.5));
                        }
                    }
                });
            }

            // acesso
            card(ui, "ACESSO", I_LINK, |ui| {
                let (icon, short, c, tip) = exposure_badge(e.exposure);
                ui.horizontal(|ui| {
                    pill(ui, format!("{icon}  {short}"), c, true);
                });
                ui.add(egui::Label::new(RichText::new(tip).color(SUBTEXT0).size(13.0)).wrap());
                ui.add_space(4.0);
                let url = e.url();
                if ui.link(RichText::new(format!("{I_GLOBE}  {url}")).size(13.5)).on_hover_text("abrir no navegador").clicked() {
                    acts.push(Act::OpenUrl(url));
                }
                if !matches!(e.exposure, Exposure::Local) {
                    if let Some(ip) = app.lan_ip {
                        let lan = format!("http://{ip}:{}", e.port);
                        let txt = RichText::new(format!("{I_GLOBE}  {lan}")).size(13.5);
                        let hint = if e.exposure.is_exposed() { "outros aparelhos da rede conseguem acessar" } else { "bloqueado pelo firewall para outros aparelhos" };
                        if ui.link(txt).on_hover_text(hint).clicked() {
                            acts.push(Act::OpenUrl(lan));
                        }
                    }
                }
                let addrs: Vec<String> = e.addrs.iter().map(|a| a.to_string()).collect();
                ui.label(RichText::new(format!("escuta em {}", addrs.join(", "))).size(12.0).color(OVERLAY0));
            });

            // tráfego
            card(ui, "TRÁFEGO", I_BOLT, |ui| {
                let data: Vec<u64> = app.hist.get(&e.key()).map(|h| h.iter().copied().collect()).unwrap_or_default();
                let peak = data.iter().copied().max().unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(rate(e.rate)).font(bold(18.0)).color(TEAL));
                    ui.label(RichText::new("agora").color(OVERLAY1).size(12.5));
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("pico {}", rate(peak as f64))).color(SUBTEXT0).size(12.5));
                    ui.label(RichText::new(format!("· {} {}", e.conns, if e.conns == 1 { "conexão" } else { "conexões" })).color(SUBTEXT0).size(12.5));
                });
                ui.add_space(4.0);
                traffic_chart(ui, &data, TEAL);
            });

            // clientes
            if e.proto == Proto::Tcp {
                let mut clients: Vec<_> = app.conns.iter().filter(|c| c.local.port() == e.port).collect();
                clients.sort_by_key(|c| {
                    let s = app.conn_stats.get(&(c.local, c.remote)).copied().unwrap_or_default();
                    std::cmp::Reverse(s.rx + s.tx)
                });
                card(ui, &format!("CLIENTES CONECTADOS ({})", clients.len()), I_USERS, |ui| {
                    if clients.is_empty() {
                        ui.label(RichText::new("Nenhum cliente conectado agora.").color(OVERLAY1));
                        return;
                    }
                    egui::Grid::new("clientes").num_columns(5).spacing([12.0, 6.0]).striped(false).show(ui, |ui| {
                        for c in clients.iter().take(12) {
                            let s = app.conn_stats.get(&(c.local, c.remote)).copied().unwrap_or_default();
                            let r = app.conn_rate.get(&(c.local, c.remote)).copied().unwrap_or(0.0);
                            let local = c.remote.ip().is_loopback();
                            ui.label(RichText::new(c.remote.to_string()).monospace().size(12.5));
                            if local {
                                pill(ui, "local", OVERLAY1, false);
                            } else {
                                pill(ui, "rede", YELLOW, true);
                            }
                            ui.label(RichText::new(format!("↓ {}", human_bytes(s.rx as f64))).size(12.5).color(SUBTEXT0));
                            ui.label(RichText::new(format!("↑ {}", human_bytes(s.tx as f64))).size(12.5).color(SUBTEXT0));
                            ui.label(RichText::new(if r >= 1.0 { rate(r) } else { s.rtt_ms.map(|x| format!("rtt {x:.1} ms")).unwrap_or_default() }).size(12.0).color(if r >= 1.0 { TEAL } else { OVERLAY0 }));
                            ui.end_row();
                        }
                    });
                    if clients.len() > 12 {
                        ui.label(RichText::new(format!("… e mais {}", clients.len() - 12)).color(OVERLAY0).size(12.0));
                    }
                });
            }

            // destinos
            if let Some(pid) = e.pid {
                let mut dest: std::collections::HashMap<String, (usize, u16)> = Default::default();
                for c in app.conns.iter().filter(|c| c.pid == Some(pid) && c.local.port() != e.port) {
                    dest.entry(c.remote.to_string()).or_insert((0, c.remote.port())).0 += 1;
                }
                if !dest.is_empty() {
                    let mut dest: Vec<_> = dest.into_iter().collect();
                    dest.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
                    card(ui, "FALA COM", I_ARROW, |ui| {
                        egui::Grid::new("destinos").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                            for (addr, (n, port)) in dest.iter().take(10) {
                                let svc = match port {
                                    5432 => "postgres",
                                    3306 => "mysql",
                                    6379 => "redis",
                                    6333 => "qdrant",
                                    7700 => "meilisearch",
                                    27017 => "mongodb",
                                    443 => "https",
                                    80 => "http",
                                    _ => "tcp",
                                };
                                pill(ui, svc, BLUE, svc != "tcp");
                                ui.label(RichText::new(addr).monospace().size(12.5));
                                ui.label(RichText::new(format!("×{n}")).color(OVERLAY1));
                                ui.end_row();
                            }
                        });
                    });
                }
            }
        });
    }

    fn activity(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Atividade").font(bold(14.0)));
            ui.label(RichText::new("portas que abriram e fecharam").size(12.0).color(OVERLAY0));
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            if self.app.events.is_empty() {
                ui.label(RichText::new("Nada ainda — suba um `pnpm dev` ou um container e ele aparece aqui.").color(OVERLAY0));
            }
            for ev in &self.app.events {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&ev.time).monospace().size(12.5).color(OVERLAY0));
                    if ev.opened {
                        pill(ui, "▲ abriu", GREEN, false);
                    } else {
                        pill(ui, "▼ fechou", RED, false);
                    }
                    ui.label(RichText::new(&ev.text).size(13.5));
                });
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some((msg, t, tone)) = &self.app.status {
                if t.elapsed() < Duration::from_secs(4) {
                    ui.label(RichText::new(msg).font(bold(13.0)).color(tone_color(*tone)));
                    return;
                }
            }
            let keys: &[(&str, &str)] = &[
                ("↑↓", "navegar"),
                ("Enter", "abrir"),
                ("C", "copiar"),
                ("E", "editor"),
                ("T", "terminal"),
                ("L", "logs"),
                ("Del", "encerrar"),
                ("R", "testar"),
                ("/", "buscar"),
                ("1-5", "visão"),
                ("U", "udp"),
                ("F1", "ajuda"),
            ];
            ui.spacing_mut().item_spacing.x = 4.0;
            for (k, d) in keys {
                ui.label(RichText::new(*k).font(bold(12.0)).color(MAUVE));
                ui.label(RichText::new(*d).size(12.0).color(OVERLAY1));
                ui.add_space(8.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("botão direito numa porta: mais ações").size(12.0).color(OVERLAY0));
            });
        });
    }

    fn modals(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let frame = |c: Color32| Frame::new().fill(MANTLE).stroke(Stroke::new(1.0, c)).corner_radius(16).inner_margin(Margin::same(24));
        match &self.app.modal {
            Modal::None => {}
            Modal::Confirm { label, .. } => {
                let label = label.clone();
                let r = egui::Modal::new(Id::new("confirmar")).frame(frame(tint(RED, 140))).show(ctx, |ui| {
                    ui.set_width(440.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(I_WARN).size(20.0).color(RED));
                        ui.label(RichText::new("Confirmar").font(bold(19.0)));
                    });
                    ui.add_space(8.0);
                    let mut t = label.chars();
                    let cap = t.next().map(|f| f.to_uppercase().collect::<String>() + t.as_str()).unwrap_or_default();
                    ui.add(egui::Label::new(RichText::new(format!("{cap}?")).size(15.0)).wrap());
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(format!("{I_STOP}  Encerrar")).color(CRUST).font(bold(14.0)))
                            .fill(RED)
                            .corner_radius(9)
                            .min_size(Vec2::new(120.0, 34.0));
                        if ui.add(b).clicked() {
                            acts.push(Act::ConfirmKill);
                        }
                        if ui.add(egui::Button::new("Cancelar").corner_radius(9).min_size(Vec2::new(100.0, 34.0))).clicked() {
                            acts.push(Act::CloseModal);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("Enter confirma · Esc cancela").size(12.0).color(OVERLAY0));
                        });
                    });
                });
                if r.should_close() {
                    acts.push(Act::CloseModal);
                }
            }
            Modal::Help => {
                let r = egui::Modal::new(Id::new("ajuda")).frame(frame(tint(MAUVE, 120))).show(ctx, |ui| {
                    ui.set_width(560.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(I_KEY).size(18.0).color(MAUVE));
                        ui.label(RichText::new("Atalhos").font(bold(19.0)));
                    });
                    ui.add_space(10.0);
                    egui::Grid::new("atalhos").num_columns(2).spacing([22.0, 8.0]).show(ui, |ui| {
                        for (k, d) in [
                            ("↑ ↓   J K", "navegar entre as portas (PgUp/PgDn, Home/End)"),
                            ("Clique", "selecionar · duplo clique abre no navegador"),
                            ("Botão direito", "menu com todas as ações da porta"),
                            ("Enter   O", "abrir http://localhost:porta"),
                            ("C", "copiar a URL"),
                            ("E", "abrir a pasta do projeto no editor"),
                            ("T", "abrir um terminal na pasta do projeto"),
                            ("L", "logs ao vivo do container docker"),
                            ("Del   X", "encerrar o processo / parar o container"),
                            ("R", "testar HTTP de novo"),
                            ("/   Ctrl+F", "buscar · Esc limpa"),
                            ("1 … 5", "tudo · dev · bancos · docker · expostas"),
                            ("U", "incluir portas UDP"),
                            ("Ctrl+Q", "sair"),
                        ] {
                            ui.label(RichText::new(k).font(bold(13.5)).color(PEACH));
                            ui.label(RichText::new(d).size(13.5));
                            ui.end_row();
                        }
                    });
                    ui.add_space(12.0);
                    ui.label(RichText::new("ACESSO").font(bold(12.0)).color(MAUVE));
                    for x in [Exposure::Local, Exposure::Blocked, Exposure::Allowed, Exposure::Docker] {
                        let (i, s, c, tip) = exposure_badge(x);
                        ui.horizontal(|ui| {
                            pill(ui, format!("{i}  {s}"), c, false);
                            ui.add(egui::Label::new(RichText::new(tip).size(12.5).color(SUBTEXT0)).wrap());
                        });
                    }
                    ui.add_space(10.0);
                    if ui.add(egui::Button::new("Fechar").corner_radius(9).min_size(Vec2::new(100.0, 32.0))).clicked() {
                        acts.push(Act::CloseModal);
                    }
                });
                if r.should_close() {
                    acts.push(Act::CloseModal);
                }
            }
        }
    }
}
