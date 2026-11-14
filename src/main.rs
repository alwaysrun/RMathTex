//! Process entry point, platform guard and window bootstrap.

// Release binaries are GUI-only: without this the process opens a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
compile_error!("RMathTex only supports Windows 10/11 x64; no fallback path exists");

mod app;
mod catalog;
mod clipboard;
mod config;
mod convert;
mod export;
mod linearmath;
mod model;
mod render;

use eframe::egui;

/// Edge length Windows requests for a title-bar icon at 100 % scaling.
const WINDOW_ICON_SIZE: usize = 32;
/// Bytes per pixel of the 32-bpp BGRA bitmaps stored in the ICO file.
const ICO_PIXEL_BYTES: usize = 4;
/// Size of the `BITMAPINFOHEADER` that precedes every bitmap in the ICO file.
const ICO_DIB_HEADER_BYTES: usize = 40;
/// Bytes of one ICO directory entry.
const ICO_DIRECTORY_ENTRY_BYTES: usize = 16;
/// Offset of the ICO directory, which follows the 6-byte file header.
const ICO_DIRECTORY_OFFSET: usize = 6;

/// Decodes the 32-bpp bitmap entry of the embedded ICO file whose size is
/// closest to [`WINDOW_ICON_SIZE`], so the title bar and taskbar stay sharp.
fn window_icon() -> egui::IconData {
    const ICO: &[u8] = include_bytes!("../assets/RMathTex.ico");
    let fallback = || egui::IconData {
        rgba: vec![0u8; 4],
        width: 1,
        height: 1,
    };
    let Some(count) = ICO
        .get(4..6)
        .map(|raw| usize::from(u16::from_le_bytes([raw[0], raw[1]])))
    else {
        return fallback();
    };
    let mut best: Option<(usize, usize)> = None;
    for index in 0..count {
        let start = ICO_DIRECTORY_OFFSET + index * ICO_DIRECTORY_ENTRY_BYTES;
        let Some(entry) = ICO.get(start..start + ICO_DIRECTORY_ENTRY_BYTES) else {
            break;
        };
        if u16::from_le_bytes([entry[6], entry[7]]) != 32 {
            continue;
        }
        let Some(offset) = read_u32(&entry[12..16]) else {
            continue;
        };
        let Some(width) = ICO.get(offset + 4..offset + 8).and_then(read_u32) else {
            continue;
        };
        let distance = width.abs_diff(WINDOW_ICON_SIZE);
        if best.is_none_or(|(_, current)| distance < current) {
            best = Some((offset, distance));
        }
    }
    let Some((offset, _)) = best else {
        return fallback();
    };
    let Some(header) = ICO.get(offset..offset + 12) else {
        return fallback();
    };
    let (width, height) = (
        read_u32(&header[4..8]).unwrap_or(0),
        read_u32(&header[8..12]).unwrap_or(0) / 2,
    );
    if width == 0 || height == 0 {
        return fallback();
    }
    let pixels = offset + ICO_DIB_HEADER_BYTES;
    let byte_len = width * height * ICO_PIXEL_BYTES;
    let Some(bitmap) = ICO.get(pixels..pixels + byte_len) else {
        return fallback();
    };
    let mut rgba = vec![0u8; byte_len];
    let row_bytes = width * ICO_PIXEL_BYTES;
    for row in 0..height {
        let source = &bitmap[(height - 1 - row) * row_bytes..][..row_bytes];
        let target = &mut rgba[row * row_bytes..][..row_bytes];
        for (pixel, out) in source
            .chunks_exact(ICO_PIXEL_BYTES)
            .zip(target.chunks_exact_mut(ICO_PIXEL_BYTES))
        {
            out.copy_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    egui::IconData {
        rgba,
        width: u32::try_from(width).unwrap_or(1),
        height: u32::try_from(height).unwrap_or(1),
    }
}

fn read_u32(bytes: &[u8]) -> Option<usize> {
    let array: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    usize::try_from(u32::from_le_bytes(array)).ok()
}

fn main() -> eframe::Result {
    // Loading once keeps the restored geometry and the in-app notice consistent.
    let startup = config::load();
    let restored = startup.as_ref().ok().and_then(|config| config.window);
    let options = eframe::NativeOptions {
        viewport: viewport_builder(restored),
        // A first start has no saved rectangle, so center on the primary
        // monitor instead of leaving the window at the OS default position.
        centered: restored.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        "RMathTex",
        options,
        Box::new(move |creation| Ok(Box::new(app::RMathTexApp::new(creation, startup)?))),
    )
}

/// Applies the persisted window rectangle when the configuration was readable.
fn viewport_builder(restored: Option<config::WindowGeometry>) -> egui::ViewportBuilder {
    let mut builder = egui::ViewportBuilder::default()
        .with_inner_size(model::WINDOW_SIZE)
        .with_min_inner_size(model::MIN_WINDOW_SIZE)
        .with_icon(window_icon());
    if let Some(window) = restored {
        builder = builder
            .with_inner_size(window.size)
            .with_position(window.position)
            .with_maximized(window.maximized);
    }
    builder
}
