use std::time::Duration;

use ratatui::widgets::Paragraph;

use super::term::{Guard, Key, read_key};
use crate::cli::Period;
use crate::ctx::Ctx;
use crate::features;
use crate::model::{self, Limit, Scan, Tool};
use crate::output;
use crate::time;

pub struct App {
    pub ctx: Ctx,
    pub scan: Scan,
    pub offline: Vec<Limit>,
    pub period: Period,
    pub now: i64,
    pub summary_now: i64,
    pub selected: usize,
    pub expanded: bool,
    pub live: bool,
    pub confirmed: bool,
    pub confirm: bool,
    pub note: Option<String>,
    pub quit: bool,
}

impl App {
    pub fn new(ctx: Ctx, live: bool) -> App {
        let now = time::now();
        let scan = output::load(&ctx, now);
        let offline = scan.limits.clone();
        let mut app = App {
            ctx,
            scan,
            offline,
            period: Period::Today,
            now,
            summary_now: now,
            selected: 0,
            expanded: false,
            live,
            confirmed: live,
            confirm: false,
            note: None,
            quit: false,
        };
        if live {
            app.fetch();
        }
        app
    }

    fn fetch(&mut self) {
        let result = features::fetch(&self.ctx, self.now);
        self.scan.limits = self.offline.clone();
        self.note = (!result.errors.is_empty()).then(|| format!("live: {}", result.errors.join(" · ")));
        model::apply_live(&mut self.scan, result);
    }

    pub fn visible(&self) -> usize {
        Tool::ALL.iter().filter(|&&t| self.scan.visible(t)).count()
    }

    fn offline_mode(&mut self) {
        self.live = false;
        self.note = None;
        self.scan.limits = self.offline.clone();
    }

    pub fn key(&mut self, key: Key) {
        if key == Key::Interrupt {
            self.quit = true;
            return;
        }
        if self.confirm {
            self.confirm = false;
            if key == Key::Char('y') {
                self.live = true;
                self.confirmed = true;
                self.fetch();
            }
            return;
        }
        match key {
            Key::Escape | Key::Interrupt | Key::Char('q') => self.quit = true,
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => self.selected = (self.selected + 1).min(self.visible().saturating_sub(1)),
            Key::Enter => self.expanded = !self.expanded,
            Key::Char(ch @ ('d' | 'w' | 'm')) => {
                self.period = match ch {
                    'd' => Period::Today,
                    'w' => Period::Week,
                    _ => Period::Month,
                };
                self.now = time::now();
                self.summary_now = self.now;
            }
            Key::Char('r') => {
                let mut fresh = App::new(self.ctx.clone(), self.live);
                fresh.period = self.period;
                fresh.selected = self.selected.min(fresh.visible().saturating_sub(1));
                fresh.expanded = self.expanded;
                fresh.confirmed = self.confirmed;
                *self = fresh;
            }
            Key::Char('l') => {
                if !features::LIVE {
                    self.note = Some("built without live feature".into());
                } else if self.live {
                    self.offline_mode();
                } else if self.confirmed {
                    self.live = true;
                    self.fetch();
                } else {
                    self.confirm = true;
                }
            }
            _ => {}
        }
    }
}

/// Logs are scanned before the terminal is touched. Redraws on each key, on resize and every 30s.
pub fn run(ctx: Ctx, live: bool) -> std::io::Result<()> {
    let mut app = App::new(ctx, live);
    let (_guard, mut term) = Guard::enter()?;
    while !app.quit {
        app.now = time::now();
        term.draw(|f| {
            let area = f.area();
            f.render_widget(Paragraph::new(super::ui::draw(&app, usize::from(area.width))), area);
        })?;
        if let Some(k) = read_key(Duration::from_secs(30))? {
            app.key(k);
        }
    }
    Ok(())
}
