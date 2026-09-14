//! Debug helper: print detected monitors. Run with `cargo run --example monitors`.
fn main() {
  match display_info::DisplayInfo::all() {
    Ok(displays) => {
      for d in displays {
        println!(
          "primary={} x={} y={} w={} h={} scale={}",
          d.is_primary, d.x, d.y, d.width, d.height, d.scale_factor
        );
      }
    }
    Err(e) => eprintln!("error: {e}"),
  }
}
