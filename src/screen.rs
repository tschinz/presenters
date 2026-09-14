//! Monitor geometry helpers (in logical points), via `display-info`.
//!
//! macOS reports monitor bounds already in logical points; other platforms report
//! physical pixels, which we convert using each monitor's scale factor.

use display_info::DisplayInfo;

#[derive(Clone, Copy, Debug)]
pub struct MonitorRect {
  pub x: f32,
  pub y: f32,
  pub w: f32,
  pub h: f32,
}

impl MonitorRect {
  /// Whether a point (logical) lies within this monitor.
  pub fn contains(&self, x: f32, y: f32) -> bool {
    x >= self.x - 1.0 && x < self.x + self.w && y >= self.y - 1.0 && y < self.y + self.h
  }
}

fn to_logical(d: &DisplayInfo) -> MonitorRect {
  let s = if d.scale_factor > 0.0 { d.scale_factor } else { 1.0 };
  if cfg!(target_os = "macos") {
    MonitorRect {
      x: d.x as f32,
      y: d.y as f32,
      w: d.width as f32,
      h: d.height as f32,
    }
  } else {
    MonitorRect {
      x: d.x as f32 / s,
      y: d.y as f32 / s,
      w: d.width as f32 / s,
      h: d.height as f32 / s,
    }
  }
}

/// The primary monitor's rectangle (where the presenter window belongs).
pub fn primary() -> Option<MonitorRect> {
  let displays = DisplayInfo::all().ok()?;
  displays.iter().find(|d| d.is_primary).or_else(|| displays.first()).map(to_logical)
}

/// The top-left of a secondary monitor, if one is attached (where the audience
/// window belongs).
pub fn external_origin() -> Option<[f32; 2]> {
  let displays = DisplayInfo::all().ok()?;
  let ext = displays.into_iter().find(|d| !d.is_primary)?;
  let r = to_logical(&ext);
  Some([r.x, r.y])
}
