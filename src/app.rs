//! Presenter dashboard.
//!
//! - **Header** (small, fixed): open, shortcuts, footer-font controls, layout flip,
//!   present/quit, timer controls, filename + page count.
//! - **Center**: current slide, next-slide preview, and notes in fixed, resizable layouts
//!   cycled by "Flip layout" (L).
//! - **Footer** (large, adjustable): slide number and elapsed time, centered.
//! - **Audience window**: a second OS window showing only the slide, on black.
//! - **Start screen**: a clickable list of recent files when nothing is open.
//!
//! All persistent state lives in `~/.config/presenter/state.json` (see [`crate::config`]).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::{self, Geometry};
use crate::document::Document;
use crate::session::{Session, format_elapsed};

/// Which piece of the presentation a panel shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PaneKind {
  Current,
  Next,
  Notes,
}

/// Which page region a cached texture holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Region {
  Slide,
  Notes,
}

const HEADER_FONT: f32 = 13.0;
const MIN_FOOTER_FONT: f32 = 12.0;
const MAX_FOOTER_FONT: f32 = 48.0;
const MIN_POINTER_SIZE: f32 = 0.25;
const MAX_POINTER_SIZE: f32 = 4.0;
const POINTER_STEP: f32 = 0.25;
const NUM_LAYOUTS: usize = 4;
const MAX_RECENT: usize = 15;
const SAVE_THROTTLE: Duration = Duration::from_millis(1200);

pub struct PresenterApp {
  document: Option<Document>,
  session: Session,
  cache: TextureCache,
  status: String,
  footer_font: f32,
  /// Laser-pointer dot size factor (1.0 = default).
  pointer_size: f32,
  layout_index: usize,
  presenting: bool,
  /// When true, the audience window shows solid black instead of the slide.
  blanked: bool,
  audience_fullscreen: bool,
  audience_geometry: Option<Geometry>,
  window_geometry: Option<Geometry>,
  recent: Vec<config::RecentEntry>,
  /// Set true when the audience window (re)opens, so we place it once and then let
  /// the user move it freely.
  audience_needs_place: bool,
  /// Pending request to fullscreen the audience window on the frame after placement
  /// (so OS fullscreen targets the monitor we just moved it to).
  audience_apply_fullscreen: bool,
  show_shortcuts: bool,
  /// Laser-pointer position, normalized (0..1) within the slide, while the mouse
  /// button is held over the presenter's current slide. `None` = no pointer shown.
  pointer_norm: Option<egui::Pos2>,
  /// Lazily-loaded faint logo shown on the start screen.
  logo: Option<egui::TextureHandle>,
  /// Persistence bookkeeping.
  dirty: bool,
  last_save: Instant,
}

impl PresenterApp {
  pub fn new(_cc: &eframe::CreationContext<'_>, initial: Option<&Path>) -> Self {
    let st = config::State::load();
    let mut app = Self {
      document: None,
      session: Session::new(0),
      cache: TextureCache::new(48),
      status: "Open a PDF to begin (O).".to_owned(),
      footer_font: st.footer_font,
      pointer_size: st.pointer_size.clamp(MIN_POINTER_SIZE, MAX_POINTER_SIZE),
      layout_index: st.layout_index % NUM_LAYOUTS,
      presenting: false,
      blanked: false,
      audience_fullscreen: st.audience_fullscreen,
      audience_geometry: st.audience_geometry,
      window_geometry: st.window_geometry,
      recent: st.recent,
      audience_needs_place: false,
      audience_apply_fullscreen: false,
      show_shortcuts: false,
      pointer_norm: None,
      logo: None,
      dirty: false,
      last_save: Instant::now(),
    };
    if let Some(path) = initial {
      app.open(path);
    }
    app
  }

  fn open(&mut self, path: &Path) {
    match Document::open(path) {
      Ok(doc) => {
        self.status = format!("notes: {:?}", doc.notes_layout());
        self.session = Session::new(doc.page_count());
        if let Some(page) = self.recent_page(path) {
          self.session.goto(page as isize);
        }
        self.document = Some(doc);
        self.cache.clear();
        self.record_recent(path);
        self.save_now(); // persist the new recent entry immediately
      }
      Err(e) => {
        self.status = format!("Failed to open {}: {e:#}", path.display());
        tracing::error!("{e:#}");
      }
    }
  }

  fn recent_page(&self, path: &Path) -> Option<usize> {
    self.recent.iter().find(|e| e.path == path).map(|e| e.page)
  }

  /// Move `path` to the front of the recent list, keeping its last page.
  fn record_recent(&mut self, path: &Path) {
    let page = self.session.current();
    let path = path.to_path_buf();
    self.recent.retain(|e| e.path != path);
    self.recent.insert(0, config::RecentEntry { path, page });
    self.recent.truncate(MAX_RECENT);
    self.dirty = true;
  }

  /// Keep the current file's recent entry pointing at the current page.
  fn sync_recent_page(&mut self) {
    let Some(path) = self.document.as_ref().map(|d| d.path().to_path_buf()) else {
      return;
    };
    let page = self.session.current();
    if let Some(entry) = self.recent.iter_mut().find(|e| e.path == path)
      && entry.page != page
    {
      entry.page = page;
      self.dirty = true;
    }
  }

  fn snapshot(&self) -> config::State {
    config::State {
      footer_font: self.footer_font,
      pointer_size: self.pointer_size,
      layout_index: self.layout_index,
      audience_fullscreen: self.audience_fullscreen,
      audience_geometry: self.audience_geometry,
      window_geometry: self.window_geometry,
      recent: self.recent.clone(),
    }
  }

  fn save_now(&mut self) {
    self.sync_recent_page();
    self.snapshot().save();
    self.last_save = Instant::now();
    self.dirty = false;
  }

  fn pick_file(&mut self) {
    if let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
      self.open(&path);
    }
  }

  fn adjust_font(&mut self, delta: f32) {
    self.footer_font = (self.footer_font + delta).clamp(MIN_FOOTER_FONT, MAX_FOOTER_FONT);
    self.dirty = true;
  }

  fn adjust_pointer(&mut self, delta: f32) {
    self.pointer_size = (self.pointer_size + delta).clamp(MIN_POINTER_SIZE, MAX_POINTER_SIZE);
    self.dirty = true;
  }

  fn flip_layout(&mut self) {
    self.layout_index = (self.layout_index + 1) % NUM_LAYOUTS;
    self.dirty = true;
  }

  fn start_presentation(&mut self) {
    if self.document.is_some() {
      self.presenting = true;
      self.audience_needs_place = true;
      // If there's a second screen, place the audience window there and fullscreen
      // it; on a single screen, open it windowed so the presenter stays visible.
      let has_external = crate::screen::external_origin().is_some();
      self.audience_apply_fullscreen = has_external;
      self.audience_fullscreen = has_external;
    }
  }

  /// Close the current document and return to the start screen.
  fn close_file(&mut self) {
    self.save_now(); // persist last page before closing
    self.document = None;
    self.presenting = false;
    self.blanked = false;
    self.pointer_norm = None;
    self.session = Session::new(0);
    self.cache.clear();
    self.status = "Open a PDF to begin (O).".to_owned();
  }

  /// Open the first PDF dropped onto the window (drag & drop).
  fn handle_dropped_files(&mut self, ctx: &egui::Context) {
    let dropped = ctx.input(|i| {
      i.raw
        .dropped_files
        .iter()
        .filter_map(|f| f.path.clone())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
    });
    if let Some(path) = dropped {
      self.open(&path);
    }
  }

  fn handle_keys(&mut self, ctx: &egui::Context) {
    let mut open = false;
    let mut font_delta = 0.0;
    let mut present = false;
    let mut close = false;
    ctx.input(|i| {
      use egui::Key;
      if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::Space) || i.key_pressed(Key::PageDown) || i.key_pressed(Key::ArrowDown) {
        self.session.next();
      }
      if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp) || i.key_pressed(Key::ArrowUp) {
        self.session.prev();
      }
      if i.key_pressed(Key::Home) {
        self.session.first();
      }
      if i.key_pressed(Key::End) {
        self.session.last();
      }
      if i.key_pressed(Key::T) {
        self.session.timer_mut().toggle();
      }
      if i.key_pressed(Key::R) {
        self.session.timer_mut().reset();
      }
      if i.key_pressed(Key::L) {
        self.flip_layout();
      }
      if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
        font_delta += 2.0;
      }
      if i.key_pressed(Key::Minus) {
        font_delta -= 2.0;
      }
      if i.key_pressed(Key::F5) {
        present = true;
      }
      if i.key_pressed(Key::Escape) {
        self.presenting = false;
      }
      if i.key_pressed(Key::B) {
        self.blanked = !self.blanked;
      }
      close = i.key_pressed(Key::W);
      open = i.key_pressed(Key::O);
    });
    if font_delta != 0.0 {
      self.adjust_font(font_delta);
    }
    if present {
      self.start_presentation();
    }
    if close {
      self.close_file();
    }
    if open {
      self.pick_file();
    }
  }

  fn header(&mut self, ctx: &egui::Context) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
      ui.horizontal_wrapped(|ui| {
        let small = |s: &str| egui::RichText::new(s).size(HEADER_FONT);

        if ui.button(small("Open (O)")).clicked() {
          self.pick_file();
        }
        if ui.button(small("Shortcuts")).clicked() {
          self.show_shortcuts = !self.show_shortcuts;
        }

        ui.separator();
        ui.label(small("Footer size"));
        if ui.button(small(" − ")).on_hover_text("Smaller (-)").clicked() {
          self.adjust_font(-2.0);
        }
        if ui.button(small(" + ")).on_hover_text("Larger (+)").clicked() {
          self.adjust_font(2.0);
        }

        ui.separator();
        ui.label(small("Pointer size"));
        if ui.button(small(" − ")).on_hover_text("Smaller pointer dot").clicked() {
          self.adjust_pointer(-POINTER_STEP);
        }
        if ui.button(small(" + ")).on_hover_text("Larger pointer dot").clicked() {
          self.adjust_pointer(POINTER_STEP);
        }

        ui.separator();
        if ui.button(small("⇄ Layout (L)")).on_hover_text("Cycle panel arrangements").clicked() {
          self.flip_layout();
        }

        if self.document.is_some() {
          ui.separator();
          if ui.button(small("✕ Close (W)")).clicked() {
            self.close_file();
          }
          if self.presenting {
            if ui.button(small("⏹ Quit (Esc)")).clicked() {
              self.presenting = false;
            }
          } else if ui.button(small("▶ Present (F5)")).clicked() {
            self.start_presentation();
          }
          let blank_label = if self.blanked { "⬛ Blanked (B)" } else { "⬛ Blank (B)" };
          if ui.button(small(blank_label)).clicked() {
            self.blanked = !self.blanked;
          }
          if ui.button(small("⏯ (T)")).clicked() {
            self.session.timer_mut().toggle();
          }
          if ui.button(small("↺ (R)")).clicked() {
            self.session.timer_mut().reset();
          }
        }

        if let Some(doc) = self.document.as_ref() {
          let name = doc.path().file_name().unwrap_or_default().to_string_lossy().into_owned();
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(small(&format!("{} · {} pages", name, doc.page_count())));
          });
        }
      });
    });
  }

  fn footer(&mut self, ctx: &egui::Context) {
    if self.document.is_none() {
      return;
    }
    let fs = self.footer_font;
    let slide = format!("Slide {} / {}", self.session.current() + 1, self.session.page_count());
    let clock = chrono::Local::now().format("%H:%M").to_string();
    let t = self.session.timer();
    let elapsed = format!("⏱ {}{}", format_elapsed(t.elapsed()), if t.is_running() { "" } else { "  (paused)" });

    egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
      ui.add_space(3.0);
      ui.vertical_centered(|ui| {
        ui.label(
          egui::RichText::new(format!("{slide}      ·      🕑 {clock}      ·      {elapsed}"))
            .size(fs)
            .strong(),
        );
      });
      ui.add_space(3.0);
    });
    // Keep the wall-clock (and running timer) display current while idle.
    ctx.request_repaint_after(Duration::from_secs(1));
  }

  fn shortcuts_window(&mut self, ctx: &egui::Context) {
    egui::Window::new("Keyboard shortcuts")
      .open(&mut self.show_shortcuts)
      .collapsible(false)
      .resizable(false)
      .show(ctx, |ui| {
        let rows = [
          ("→ / Space / PgDn", "Next slide"),
          ("← / PgUp", "Previous slide"),
          ("Home / End", "First / last slide"),
          ("F5", "Start presentation"),
          ("Esc", "Quit presentation"),
          ("B", "Blank the audience screen (black)"),
          ("W", "Close file, back to start screen"),
          ("T", "Timer pause / resume"),
          ("R", "Reset timer"),
          ("L", "Flip layout (H/V with no notes; 4 presets with notes)"),
          ("+ / −", "Footer font larger / smaller"),
          ("O", "Open a PDF"),
          ("Hold mouse on current slide", "Laser pointer (dot on the audience screen)"),
          ("(audience) double-click", "Toggle fullscreen"),
        ];
        egui::Grid::new("shortcuts-grid").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
          for (k, d) in rows {
            ui.strong(k);
            ui.label(d);
            ui.end_row();
          }
        });
      });
  }

  /// Lazily load the faint start-screen logo texture.
  fn logo_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
    self
      .logo
      .get_or_insert_with(|| {
        let bytes = include_bytes!("../img/logo-gray.png");
        let color = match image::load_from_memory(bytes) {
          Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw())
          }
          Err(_) => egui::ColorImage::new([1, 1], egui::Color32::TRANSPARENT),
        };
        ctx.load_texture("start-logo", color, egui::TextureOptions::LINEAR)
      })
      .clone()
  }

  /// The screen shown when no document is open: a faint logo watermark plus a
  /// recent-files list to click, or a prompt to open one.
  fn start_screen(&mut self, ctx: &egui::Context) {
    let logo = self.logo_texture(ctx);
    let mut to_open: Option<PathBuf> = None;
    egui::CentralPanel::default().show(ctx, |ui| {
      ui.add_space(28.0);
      ui.vertical_centered(|ui| {
        ui.heading("presenters");
        ui.add_space(10.0);
        // Faint gray logo, just below the title.
        let [lw, lh] = logo.size();
        let aspect = lw as f32 / lh.max(1) as f32;
        let w = 220.0_f32.min(ui.available_width() * 0.6);
        let h = w / aspect;
        ui.add(egui::Image::new(egui::load::SizedTexture::new(logo.id(), egui::vec2(w, h))).tint(egui::Color32::from_white_alpha(90)));
        ui.add_space(12.0);
        if ui.button("Open a PDF… (O)").clicked() {
          self.pick_file();
        }
        ui.add_space(4.0);
        ui.weak("… or drag & drop a PDF onto the window");
      });
      ui.add_space(16.0);

      if self.recent.is_empty() {
        ui.vertical_centered(|ui| ui.weak("No recent files yet."));
        return;
      }

      ui.separator();
      ui.add_space(8.0);
      ui.vertical_centered(|ui| ui.strong(egui::RichText::new("Recent files").size(16.0)));
      ui.add_space(10.0);

      // Bigger file buttons, centered, at most three per row.
      let cols = self.recent.len().clamp(1, 3);
      let (btn_w, btn_h) = (260.0_f32, 52.0_f32);
      let gap = 12.0_f32;
      egui::ScrollArea::vertical().show(ui, |ui| {
        let avail = ui.available_width();
        let row_w = cols as f32 * btn_w + (cols as f32 - 1.0) * gap;
        let pad = ((avail - row_w) * 0.5).max(0.0);
        for chunk in self.recent.chunks(cols) {
          ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.spacing_mut().item_spacing.x = gap;
            for entry in chunk {
              let name = entry.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
              let folder = entry.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
              let resp = ui.add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new(format!("📄  {name}")).size(17.0)));
              if resp.clicked() {
                to_open = Some(entry.path.clone());
              }
              resp.on_hover_text(&folder);
            }
          });
          ui.add_space(gap);
        }
      });
    });
    if let Some(path) = to_open {
      self.open(&path);
    }
  }

  /// Track the presenter window's geometry for persistence.
  fn capture_window_geometry(&mut self, ctx: &egui::Context) {
    let (pos, size) = ctx.input(|i| {
      let vp = i.viewport();
      (vp.outer_rect.map(|r| r.min), vp.inner_rect.map(|r| r.size()))
    });
    if let (Some(p), Some(s)) = (pos, size) {
      let g = Geometry {
        x: p.x,
        y: p.y,
        w: s.x,
        h: s.y,
      };
      if geom_changed(self.window_geometry, g) {
        self.window_geometry = Some(g);
        self.dirty = true;
      }
    }
  }

  /// Lay out the panels for the current arrangement. Dividers are draggable (sizes are
  /// remembered by egui); the arrangement is chosen by "Flip layout" (L). With no notes:
  /// just current + next (L toggles horizontal/vertical).
  fn layout(&mut self, ctx: &egui::Context) {
    let Self {
      document,
      session,
      cache,
      layout_index,
      pointer_norm,
      pointer_size,
      ..
    } = self;
    let pointer_size = *pointer_size;
    let doc = document.as_ref();
    let screen = ctx.screen_rect();
    let has_notes = doc.map(|d| d.notes_region().is_some()).unwrap_or(false);

    if !has_notes {
      if *layout_index % 2 == 0 {
        egui::SidePanel::right("nn-next-h")
          .resizable(true)
          .default_width(screen.width() * 0.5)
          .show(ctx, |ui| {
            render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      } else {
        egui::TopBottomPanel::bottom("nn-next-v")
          .resizable(true)
          .default_height(screen.height() * 0.45)
          .show(ctx, |ui| {
            render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      }
      return;
    }

    match *layout_index % NUM_LAYOUTS {
      0 => {
        egui::SidePanel::right("side")
          .resizable(true)
          .default_width(screen.width() * 0.34)
          .show(ctx, |ui| {
            egui::TopBottomPanel::top("side-top")
              .resizable(true)
              .default_height(ui.available_height() * 0.5)
              .show_inside(ui, |ui| {
                render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
              });
            egui::CentralPanel::default().show_inside(ui, |ui| {
              render_pane(ui, ctx, cache, doc, session, PaneKind::Notes, None);
            });
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      }
      1 => {
        egui::SidePanel::right("side")
          .resizable(true)
          .default_width(screen.width() * 0.34)
          .show(ctx, |ui| {
            egui::TopBottomPanel::top("side-top")
              .resizable(true)
              .default_height(ui.available_height() * 0.5)
              .show_inside(ui, |ui| {
                render_pane(ui, ctx, cache, doc, session, PaneKind::Notes, None);
              });
            egui::CentralPanel::default().show_inside(ui, |ui| {
              render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
            });
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      }
      2 => {
        egui::SidePanel::left("col-next")
          .resizable(true)
          .default_width(screen.width() * 0.24)
          .show(ctx, |ui| {
            render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
          });
        egui::SidePanel::right("col-notes")
          .resizable(true)
          .default_width(screen.width() * 0.28)
          .show(ctx, |ui| {
            render_pane(ui, ctx, cache, doc, session, PaneKind::Notes, None);
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      }
      _ => {
        egui::TopBottomPanel::bottom("row")
          .resizable(true)
          .default_height(screen.height() * 0.32)
          .show(ctx, |ui| {
            egui::SidePanel::left("row-next")
              .resizable(true)
              .default_width(ui.available_width() * 0.5)
              .show_inside(ui, |ui| {
                render_pane(ui, ctx, cache, doc, session, PaneKind::Next, None);
              });
            egui::CentralPanel::default().show_inside(ui, |ui| {
              render_pane(ui, ctx, cache, doc, session, PaneKind::Notes, None);
            });
          });
        egui::CentralPanel::default().show(ctx, |ui| {
          render_pane(ui, ctx, cache, doc, session, PaneKind::Current, Some((pointer_size, &mut *pointer_norm)));
        });
      }
    }
  }
}

impl eframe::App for PresenterApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    self.handle_keys(ctx);
    self.handle_dropped_files(ctx);

    if self.session.timer().is_running() {
      ctx.request_repaint_after(Duration::from_millis(250));
    }

    self.header(ctx);
    self.footer(ctx);
    self.shortcuts_window(ctx);

    if self.document.is_none() {
      self.start_screen(ctx);
    } else {
      self.layout(ctx);
      self.sync_recent_page();

      if let Some(n) = self.session.next_page()
        && let Some(doc) = self.document.as_ref()
      {
        self.cache.get_or_render(ctx, doc, n, Region::Slide, [1280, 720]);
      }

      if self.presenting {
        self.show_audience(ctx);
      }
    }

    self.capture_window_geometry(ctx);

    // Throttled persistence; schedule a wake so a change flushes even while idle.
    if self.dirty && self.last_save.elapsed() >= SAVE_THROTTLE {
      self.save_now();
    }
    if self.dirty {
      ctx.request_repaint_after(SAVE_THROTTLE + Duration::from_millis(100));
    }
  }

  fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
    self.save_now();
  }
}

impl PresenterApp {
  /// The audience window: only the slide, on black. Double-click toggles fullscreen;
  /// Esc or the close button quits. Geometry is captured for persistence.
  fn show_audience(&mut self, ctx: &egui::Context) {
    let vid = egui::ViewportId::from_hash_of("audience-window");

    let mut builder = egui::ViewportBuilder::default().with_title("Presentation").with_icon(crate::app_icon());
    if self.audience_needs_place {
      // Place the window this frame: on the external monitor if present, else at the
      // last-used position. Fullscreen (if wanted) is applied next frame, once the
      // window is on the target monitor, so it fullscreens there and not on primary.
      if let Some([mx, my]) = crate::screen::external_origin() {
        builder = builder.with_position([mx + 80.0, my + 80.0]).with_inner_size([800.0, 450.0]);
      } else if let Some(g) = self.audience_geometry {
        builder = builder.with_position([g.x, g.y]).with_inner_size([g.w, g.h]);
      } else {
        builder = builder.with_inner_size([960.0, 540.0]);
      }
    }

    let mut quit = false;
    let mut toggle_fullscreen = false;

    ctx.show_viewport_immediate(vid, builder, |vctx, _class| {
      egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(egui::Color32::BLACK))
        .show(vctx, |ui| {
          let resp = ui.interact(ui.max_rect(), ui.id().with("audience-surface"), egui::Sense::click());
          if resp.double_clicked() {
            toggle_fullscreen = true;
          }
          // When blanked, leave the black frame empty.
          if !self.blanked
            && let Some(doc) = self.document.as_ref()
          {
            let page = self.session.current();
            let aspect = doc.slide_aspect(page);
            let rect = draw_slide_region(ui, vctx, &mut self.cache, doc, page, Region::Slide, aspect);
            // Laser pointer dot, mirrored from the presenter's current slide.
            if let (Some(rect), Some(n)) = (rect, self.pointer_norm) {
              draw_pointer_dot(ui.painter(), rect, n, self.pointer_size);
            }
          }
        });

      let (pos, size) = vctx.input(|i| {
        let vp = i.viewport();
        (vp.outer_rect.map(|r| r.min), vp.inner_rect.map(|r| r.size()))
      });
      if !self.audience_fullscreen
        && let (Some(p), Some(s)) = (pos, size)
      {
        let g = Geometry {
          x: p.x,
          y: p.y,
          w: s.x,
          h: s.y,
        };
        if geom_changed(self.audience_geometry, g) {
          self.audience_geometry = Some(g);
          self.dirty = true;
        }
      }

      if vctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        quit = true;
      }
    });

    // Staged placement: frame 1 positions the window on the target monitor; frame 2
    // fullscreens it there (so OS fullscreen picks the monitor we moved it to).
    if self.audience_needs_place {
      self.audience_needs_place = false;
      if self.audience_apply_fullscreen {
        ctx.request_repaint(); // ensure a second frame runs to apply fullscreen
      }
    } else if self.audience_apply_fullscreen {
      self.audience_apply_fullscreen = false;
      self.audience_fullscreen = true;
      ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Fullscreen(true));
    }

    if toggle_fullscreen {
      self.audience_fullscreen = !self.audience_fullscreen;
      self.dirty = true;
      ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Fullscreen(self.audience_fullscreen));
    }
    if quit {
      self.presenting = false;
    }
  }
}

/// True if `new` differs from `old` by more than a pixel (or `old` is unset).
fn geom_changed(old: Option<Geometry>, new: Geometry) -> bool {
  match old {
    None => true,
    Some(o) => (o.x - new.x).abs() > 1.0 || (o.y - new.y).abs() > 1.0 || (o.w - new.w).abs() > 1.0 || (o.h - new.h).abs() > 1.0,
  }
}

/// Render one panel's content (a titled, aspect-fitted page region).
///
/// When `pointer` is `Some` (the current-slide pane), the slide becomes a laser-pointer
/// surface: while the mouse button is held over it, the normalized cursor position is
/// written to the slot (and a dot is drawn here too as presenter feedback); releasing
/// clears it.
fn render_pane(
  ui: &mut egui::Ui,
  ctx: &egui::Context,
  cache: &mut TextureCache,
  doc: Option<&Document>,
  session: &Session,
  kind: PaneKind,
  pointer: Option<(f32, &mut Option<egui::Pos2>)>,
) {
  ui.small(match kind {
    PaneKind::Current => "Current",
    PaneKind::Next => "Next",
    PaneKind::Notes => "Notes",
  });
  let Some(doc) = doc else { return };
  let current = session.current();

  let (page, region, aspect) = match kind {
    PaneKind::Current => (Some(current), Region::Slide, Some(doc.slide_aspect(current))),
    PaneKind::Next => match session.next_page() {
      Some(n) => (Some(n), Region::Slide, Some(doc.slide_aspect(n))),
      None => (None, Region::Slide, None),
    },
    PaneKind::Notes => match doc.notes_aspect(current) {
      Some(a) => (Some(current), Region::Notes, Some(a)),
      None => (None, Region::Notes, None),
    },
  };

  match (page, aspect) {
    (Some(p), Some(a)) => {
      let rect = draw_slide_region(ui, ctx, cache, doc, p, region, a);
      if let (Some((size, slot)), Some(rect)) = (pointer, rect) {
        let resp = ui.interact(rect, ui.id().with("laser"), egui::Sense::click_and_drag());
        if resp.is_pointer_button_down_on()
          && let Some(pos) = resp.interact_pointer_pos()
        {
          let nx = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
          let ny = ((pos.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
          *slot = Some(egui::pos2(nx, ny));
        } else if !resp.is_pointer_button_down_on() {
          *slot = None;
        }
        if let Some(n) = *slot {
          draw_pointer_dot(ui.painter(), rect, n, size);
        }
      }
    }
    _ => {
      let msg = match kind {
        PaneKind::Next => "— end —",
        PaneKind::Notes => "No notes in this document.",
        PaneKind::Current => "",
      };
      ui.centered_and_justified(|ui| {
        ui.weak(msg);
      });
    }
  }
}

/// Draw the laser-pointer dot at a normalized position (0..1) within `rect`,
/// scaled by `size` (1.0 = default).
fn draw_pointer_dot(painter: &egui::Painter, rect: egui::Rect, norm: egui::Pos2, size: f32) {
  let center = egui::pos2(rect.left() + norm.x * rect.width(), rect.top() + norm.y * rect.height());
  let r = (rect.height() * 0.012 * size).max(3.0);
  // Semi-transparent: a faint halo and a translucent red core.
  painter.circle_filled(center, r * 2.2, egui::Color32::from_rgba_unmultiplied(255, 40, 40, 40));
  painter.circle_filled(center, r, egui::Color32::from_rgba_unmultiplied(230, 30, 30, 140));
  painter.circle_stroke(center, r, egui::Stroke::new(1.5_f32, egui::Color32::from_rgba_unmultiplied(150, 0, 0, 140)));
}

/// Draw an aspect-fitted image of a page region into the available area (letterboxed).
/// Returns the on-screen rect the image occupies (for overlays such as the pointer dot).
fn draw_slide_region(
  ui: &mut egui::Ui,
  ctx: &egui::Context,
  cache: &mut TextureCache,
  doc: &Document,
  page: usize,
  region: Region,
  aspect: f32,
) -> Option<egui::Rect> {
  let full = ui.available_rect_before_wrap();
  let ppp = ctx.pixels_per_point();

  let (mut w, mut h) = (full.width(), full.width() / aspect);
  if h > full.height() {
    h = full.height();
    w = full.height() * aspect;
  }
  let img_rect = egui::Rect::from_center_size(full.center(), egui::vec2(w, h));
  let px = [(w * ppp).round().max(1.0) as u32, (h * ppp).round().max(1.0) as u32];

  if let Some(tex) = cache.get_or_render(ctx, doc, page, region, px) {
    ui.put(img_rect, egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(w, h))));
    Some(img_rect)
  } else {
    ui.put(img_rect, egui::Spinner::new());
    None
  }
}

/// Bounded texture cache keyed by (page, region, pixel width). Evicts oldest entries.
struct TextureCache {
  map: HashMap<(usize, Region, u32), egui::TextureHandle>,
  order: VecDeque<(usize, Region, u32)>,
  capacity: usize,
}

impl TextureCache {
  fn new(capacity: usize) -> Self {
    Self {
      map: HashMap::new(),
      order: VecDeque::new(),
      capacity,
    }
  }

  fn clear(&mut self) {
    self.map.clear();
    self.order.clear();
  }

  fn get_or_render(&mut self, ctx: &egui::Context, doc: &Document, page: usize, region: Region, target_px: [u32; 2]) -> Option<egui::TextureHandle> {
    let key = (page, region, target_px[0]);
    if let Some(tex) = self.map.get(&key) {
      return Some(tex.clone());
    }

    let img = match region {
      Region::Slide => doc.render_slide(page, target_px),
      Region::Notes => match doc.render_notes(page, target_px) {
        Ok(Some(img)) => Ok(img),
        Ok(None) => return None,
        Err(e) => Err(e),
      },
    };
    let img = match img {
      Ok(img) => img,
      Err(e) => {
        tracing::error!("render {region:?} page {page}: {e:#}");
        return None;
      }
    };

    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture(format!("{region:?}-{page}-{}", target_px[0]), color, egui::TextureOptions::LINEAR);

    self.map.insert(key, tex.clone());
    self.order.push_back(key);
    while self.order.len() > self.capacity {
      if let Some(old) = self.order.pop_front() {
        self.map.remove(&old);
      }
    }
    Some(tex)
  }
}
