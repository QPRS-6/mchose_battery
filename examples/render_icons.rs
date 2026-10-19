//! Render the tray icon states to PPM files for visual inspection.
//!
//! ```sh
//! cargo run --example render_icons
//! magick target/icon-preview/*.ppm target/icon-preview/preview.png
//! ```

use std::io;

use mchose_battery::icon;
use mchose_battery::types::ChargeState;

fn main() -> io::Result<()> {
    let directory = std::path::Path::new("target/icon-preview");
    std::fs::create_dir_all(directory)?;

    let states = [
        ("00-low", Some(5), ChargeState::NotCharging, true, false),
        ("20-amber", Some(20), ChargeState::NotCharging, true, false),
        ("55-green", Some(55), ChargeState::NotCharging, true, false),
        ("85-charging", Some(85), ChargeState::Charging, true, false),
        ("100-full", Some(100), ChargeState::Full, true, false),
        ("unknown", None, ChargeState::Unknown, false, false),
        ("stale", Some(72), ChargeState::NotCharging, true, true),
    ];

    for (name, percent, charge, connected, stale) in states {
        let rgba = icon::render_rgba(percent, charge, connected, stale);
        let mut ppm = Vec::with_capacity(32 * 32 * 3 + 16);
        ppm.extend_from_slice(b"P6\n32 32\n255\n");
        for pixel in rgba.chunks(4) {
            let alpha = pixel[3] as u32;
            for channel in pixel.iter().take(3) {
                let value = (*channel as u32 * alpha + 245 * (255 - alpha)) / 255;
                ppm.push(value as u8);
            }
        }
        std::fs::write(directory.join(format!("{name}.ppm")), ppm)?;
    }

    println!("Wrote icon previews to {}", directory.display());
    Ok(())
}
