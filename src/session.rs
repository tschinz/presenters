//! Presentation state: the current slide index and the talk timer.

use std::time::{Duration, Instant};

pub struct Session {
  current: usize,
  page_count: usize,
  timer: Timer,
}

impl Session {
  pub fn new(page_count: usize) -> Self {
    Self {
      current: 0,
      page_count,
      timer: Timer::new(),
    }
  }

  pub fn current(&self) -> usize {
    self.current
  }

  pub fn page_count(&self) -> usize {
    self.page_count
  }

  /// The page after the current one, if any (for the "next slide" preview).
  pub fn next_page(&self) -> Option<usize> {
    (self.current + 1 < self.page_count).then_some(self.current + 1)
  }

  /// Clamp and set the current slide.
  pub fn goto(&mut self, page: isize) {
    if self.page_count == 0 {
      return;
    }
    self.current = page.clamp(0, self.page_count as isize - 1) as usize;
  }

  pub fn next(&mut self) {
    self.goto(self.current as isize + 1);
  }

  pub fn prev(&mut self) {
    self.goto(self.current as isize - 1);
  }

  pub fn first(&mut self) {
    self.goto(0);
  }

  pub fn last(&mut self) {
    self.goto(self.page_count as isize - 1);
  }

  pub fn timer(&self) -> &Timer {
    &self.timer
  }

  pub fn timer_mut(&mut self) -> &mut Timer {
    &mut self.timer
  }
}

/// A pausable stopwatch.
pub struct Timer {
  /// When running, the instant it (re)started; `None` when paused.
  started: Option<Instant>,
  /// Time banked from previous run stretches.
  accumulated: Duration,
}

impl Timer {
  pub fn new() -> Self {
    Self {
      started: None,
      accumulated: Duration::ZERO,
    }
  }

  pub fn is_running(&self) -> bool {
    self.started.is_some()
  }

  pub fn elapsed(&self) -> Duration {
    match self.started {
      Some(t) => self.accumulated + t.elapsed(),
      None => self.accumulated,
    }
  }

  /// Start/resume or pause.
  pub fn toggle(&mut self) {
    match self.started.take() {
      Some(t) => self.accumulated += t.elapsed(),  // was running -> pause
      None => self.started = Some(Instant::now()), // was paused -> resume
    }
  }

  pub fn start(&mut self) {
    if self.started.is_none() {
      self.started = Some(Instant::now());
    }
  }

  /// Zero the elapsed time. If the timer was running it keeps running (restarts from
  /// zero); if paused it stays paused at zero.
  pub fn reset(&mut self) {
    self.accumulated = Duration::ZERO;
    if self.started.is_some() {
      self.started = Some(Instant::now());
    }
  }
}

impl Default for Timer {
  fn default() -> Self {
    Self::new()
  }
}

/// Format a duration as `H:MM:SS` (hours dropped when zero: `M:SS`).
pub fn format_elapsed(d: Duration) -> String {
  let secs = d.as_secs();
  let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
  if h > 0 {
    format!("{h}:{m:02}:{s:02}")
  } else {
    format!("{m}:{s:02}")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reset_zeroes_but_keeps_running_state() {
    let mut t = Timer::new();
    t.start();
    assert!(t.is_running());
    t.reset();
    // Still running, but back near zero.
    assert!(t.is_running());
    assert!(t.elapsed() < Duration::from_millis(50));

    t.toggle(); // pause
    assert!(!t.is_running());
    t.reset();
    assert!(!t.is_running());
    assert_eq!(t.elapsed(), Duration::ZERO);
  }
}
