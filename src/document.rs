//! An open presentation document: the renderer plus derived metadata (page count,
//! sizes, detected notes layout).

use std::path::{Path, PathBuf};

use anyhow::Result;
use image::RgbaImage;

use crate::render::pdfium::PdfiumRenderer;
use crate::render::{PageRenderer, PageSize};

/// Where the speaker notes live relative to the slide on each page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesLayout {
  /// One slide per page, no notes.
  None,
  /// Page is a slide + notes side by side / stacked. The variant names the notes half.
  Split(NotesSide),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesSide {
  Right,
  Left,
  Top,
  Bottom,
}

/// Normalized sub-rectangle of a page (0..1 in both axes).
#[derive(Clone, Copy, Debug)]
pub struct Region {
  pub x: f32,
  pub y: f32,
  pub w: f32,
  pub h: f32,
}

impl Region {
  pub const FULL: Region = Region {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
  };
}

pub struct Document {
  path: PathBuf,
  renderer: Box<dyn PageRenderer>,
  notes: NotesLayout,
}

impl Document {
  pub fn open(path: &Path) -> Result<Self> {
    let renderer = PdfiumRenderer::open(path)?;
    let notes = detect_notes_layout(&renderer);
    tracing::info!(?notes, pages = renderer.page_count(), "opened document");
    Ok(Self {
      path: path.to_path_buf(),
      renderer: Box::new(renderer),
      notes,
    })
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn page_count(&self) -> usize {
    self.renderer.page_count()
  }

  pub fn page_size(&self, page: usize) -> PageSize {
    self.renderer.page_size(page)
  }

  pub fn notes_layout(&self) -> NotesLayout {
    self.notes
  }

  /// The slide sub-rectangle of a page (the part shown to the audience).
  pub fn slide_region(&self) -> Region {
    match self.notes {
      NotesLayout::None => Region::FULL,
      NotesLayout::Split(side) => slide_side(side),
    }
  }

  /// The notes sub-rectangle, if any.
  pub fn notes_region(&self) -> Option<Region> {
    match self.notes {
      NotesLayout::None => None,
      NotesLayout::Split(side) => Some(notes_side(side)),
    }
  }

  /// Aspect (w/h) of the slide portion of a page.
  pub fn slide_aspect(&self, page: usize) -> f32 {
    region_aspect(self.page_size(page).aspect(), self.slide_region())
  }

  /// Aspect (w/h) of the notes portion of a page, if there are notes.
  pub fn notes_aspect(&self, page: usize) -> Option<f32> {
    self.notes_region().map(|r| region_aspect(self.page_size(page).aspect(), r))
  }

  /// Render a full page at the given pixel size.
  pub fn render_page(&self, page: usize, target_px: [u32; 2]) -> Result<RgbaImage> {
    self.renderer.render(page, target_px)
  }

  /// Render just the slide portion of a page (the audience view).
  pub fn render_slide(&self, page: usize, target_px: [u32; 2]) -> Result<RgbaImage> {
    self.render_region(page, self.slide_region(), target_px)
  }

  /// Render just the notes portion of a page, if any.
  pub fn render_notes(&self, page: usize, target_px: [u32; 2]) -> Result<Option<RgbaImage>> {
    match self.notes_region() {
      Some(region) => Ok(Some(self.render_region(page, region, target_px)?)),
      None => Ok(None),
    }
  }

  /// Render an arbitrary normalized sub-rectangle of a page at `target_px`.
  ///
  /// The whole page is rasterized at a resolution that makes `region` land on
  /// `target_px`, then cropped — so the returned image is at full slide resolution,
  /// not an upscale of a small render.
  fn render_region(&self, page: usize, region: Region, target_px: [u32; 2]) -> Result<RgbaImage> {
    // Fast path: the whole page.
    if region.x <= f32::EPSILON && region.y <= f32::EPSILON && region.w >= 1.0 - f32::EPSILON && region.h >= 1.0 - f32::EPSILON {
      return self.renderer.render(page, target_px);
    }

    let [tw, th] = [target_px[0].max(1), target_px[1].max(1)];
    let full_w = (tw as f32 / region.w).round().max(1.0) as u32;
    let full_h = (th as f32 / region.h).round().max(1.0) as u32;
    let full = self.renderer.render(page, [full_w, full_h])?;

    let x = (region.x * full.width() as f32).round() as u32;
    let y = (region.y * full.height() as f32).round() as u32;
    let w = tw.min(full.width().saturating_sub(x)).max(1);
    let h = th.min(full.height().saturating_sub(y)).max(1);
    Ok(image::imageops::crop_imm(&full, x, y, w, h).to_image())
  }
}

fn region_aspect(page_aspect: f32, r: Region) -> f32 {
  if r.h > 0.0 {
    page_aspect * (r.w / r.h)
  } else {
    page_aspect
  }
}

/// The slide occupies the half opposite the notes.
fn slide_side(notes: NotesSide) -> Region {
  match notes {
    NotesSide::Right => Region {
      x: 0.0,
      y: 0.0,
      w: 0.5,
      h: 1.0,
    },
    NotesSide::Left => Region {
      x: 0.5,
      y: 0.0,
      w: 0.5,
      h: 1.0,
    },
    NotesSide::Bottom => Region {
      x: 0.0,
      y: 0.0,
      w: 1.0,
      h: 0.5,
    },
    NotesSide::Top => Region {
      x: 0.0,
      y: 0.5,
      w: 1.0,
      h: 0.5,
    },
  }
}

fn notes_side(notes: NotesSide) -> Region {
  match notes {
    NotesSide::Right => Region {
      x: 0.5,
      y: 0.0,
      w: 0.5,
      h: 1.0,
    },
    NotesSide::Left => Region {
      x: 0.0,
      y: 0.0,
      w: 0.5,
      h: 1.0,
    },
    NotesSide::Bottom => Region {
      x: 0.0,
      y: 0.5,
      w: 1.0,
      h: 0.5,
    },
    NotesSide::Top => Region {
      x: 0.0,
      y: 0.0,
      w: 1.0,
      h: 0.5,
    },
  }
}

/// Detect split-notes pages by aspect ratio.
///
/// A normal slide is ~4:3 (1.33) or 16:9 (1.78). Beamer/Typst "notes on second
/// screen=right" doubles the width, giving ~2.67 or ~3.56. If the first page is
/// roughly twice as wide as a plausible slide, treat it as right-side split notes.
fn detect_notes_layout(r: &impl PageRenderer) -> NotesLayout {
  if r.page_count() == 0 {
    return NotesLayout::None;
  }
  let aspect = r.page_size(0).aspect();
  // Widest normal slide is 16:9 ≈ 1.78; anything past ~2.4 is a doubled page.
  if aspect >= 2.4 {
    NotesLayout::Split(NotesSide::Right)
  } else {
    NotesLayout::None
  }
}
