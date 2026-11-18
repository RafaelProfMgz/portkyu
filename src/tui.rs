//! Interface de terminal (ratatui): laço principal e teclado.
use crate::app::{App, Modal, Tone, VIEWS};
use crate::ui;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

pub fn run(app: &mut App) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let res = (|| {
        while !app.quit {
            terminal.draw(|f| ui::draw(f, app))?;
            if event::poll(Duration::from_millis(150))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        handle_key(app, k);
                    }
                }
            }
            if app.last_refresh.elapsed() >= Duration::from_millis(1000) {
                app.refresh();
            }
        }
        Ok(())
    })();
    ratatui::restore();
    res
}

// ─── Teclado ────────────────────────────────────────────────────────────
pub fn handle_key(app: &mut App, k: KeyEvent) {
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        app.quit = true;
        return;
    }
    match std::mem::replace(&mut app.modal, Modal::None) {
        Modal::Help => return,
        Modal::Confirm { label, target } => {
            if matches!(k.code, KeyCode::Char('s' | 'S' | 'y' | 'Y') | KeyCode::Enter) {
                app.kill(target, &label);
            } else {
                app.status("cancelado", Tone::Muted);
            }
            return;
        }
        Modal::None => {}
    }
    if app.filtering {
        match k.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.filtering = false;
            }
            KeyCode::Enter => app.filtering = false,
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(c) => app.filter.push(c),
            KeyCode::Down => app.move_sel(1),
            KeyCode::Up => app.move_sel(-1),
            _ => {}
        }
        app.rebuild_visible();
        return;
    }
    match k.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Esc => {
            app.filter.clear();
            app.rebuild_visible();
        }
        KeyCode::Down | KeyCode::Char('j') => app.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_sel(-1),
        KeyCode::PageDown => app.move_sel(10),
        KeyCode::PageUp => app.move_sel(-10),
        KeyCode::Home | KeyCode::Char('g') => app.move_sel(-10_000),
        KeyCode::End | KeyCode::Char('G') => app.move_sel(10_000),
        KeyCode::Char(c @ '1'..='5') => {
            app.view = VIEWS[(c as u8 - b'1') as usize].0;
            app.rebuild_visible();
        }
        KeyCode::Tab => {
            let i = VIEWS.iter().position(|(v, _)| *v == app.view).unwrap_or(0);
            app.view = VIEWS[(i + 1) % VIEWS.len()].0;
            app.rebuild_visible();
        }
        KeyCode::BackTab => {
            let i = VIEWS.iter().position(|(v, _)| *v == app.view).unwrap_or(0);
            app.view = VIEWS[(i + VIEWS.len() - 1) % VIEWS.len()].0;
            app.rebuild_visible();
        }
        KeyCode::Char('/') => app.filtering = true,
        KeyCode::Char('u') => {
            app.show_udp = !app.show_udp;
            app.status(if app.show_udp { "mostrando TCP + UDP" } else { "mostrando só TCP" }, Tone::Info);
            app.refresh();
        }
        KeyCode::Char('?') => app.modal = Modal::Help,
        KeyCode::Char('o') | KeyCode::Enter => app.open_browser(),
        KeyCode::Char('c') => app.copy_url(),
        KeyCode::Char('e') => app.open_editor(),
        KeyCode::Char('t') => app.open_terminal(),
        KeyCode::Char('l') => app.open_logs(),
        KeyCode::Char('r') => {
            if let Some(p) = app.selected().map(|e| e.port) {
                app.prober.request(p);
                app.status(format!("testando http://localhost:{p} …"), Tone::Info);
            }
        }
        KeyCode::Char('x') | KeyCode::Delete => app.ask_kill(),
        _ => {}
    }
}
