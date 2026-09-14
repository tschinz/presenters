//! Rendering abstraction.
//!
//! Everything above this module talks to PDFs through the [`PageRenderer`] trait,
//! never through PDFium types directly. Swapping the backend (MuPDF, a pure-Rust
//! renderer, ...) means adding a new impl here and nothing else.

pub mod pdfium;

use image::RgbaImage;

/// A page's intrinsic size, in PDF points (1/72 inch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
  pub width: f32,
  pub height: f32,
}

impl PageSize {
  /// width / height. Guards against degenerate pages.
  pub fn aspect(&self) -> f32 {
    if self.height > 0.0 {
      self.width / self.height
    } else {
      1.0
    }
  }
}

/// Opens one document and rasterizes its pages to RGBA bitmaps.
///
/// Implementations own the document for their lifetime; callers address pages by
/// zero-based index and ask for a pixel size (already aspect-correct — the caller
/// decides the fit).
pub trait PageRenderer {
  fn page_count(&self) -> usize;

  /// Intrinsic size of a page, in points.
  fn page_size(&self, page: usize) -> PageSize;

  /// Rasterize `page` at `target_px` = [width, height] pixels.
  fn render(&self, page: usize, target_px: [u32; 2]) -> anyhow::Result<RgbaImage>;
}
