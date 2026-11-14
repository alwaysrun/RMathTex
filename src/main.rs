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

/// Decodes the single 32-bpp BMP entry of the embedded ICO file.
fn window_icon() -> egui::IconData {
    const ICO: &[u8] = include_bytes!("../assets/icon.ico");
    let fallback = || egui::IconData {
        rgba: vec![0u8; 4],
        width: 1,
        height: 1,
    };
    let Some(offset) = ICO.get(18..22).and_then(read_u32) else {
        return fallback();
    };
    let Some(header) = ICO.get(offset..offset + 12) else {
        return fallback();
    };
    let (width, height) = (
        read_u32(&header[4..8]).unwrap_or(0),
        read_u32(&header[8..12]).unwrap_or(0) / 2,
    );
    let pixels = offset + 40;
    let Some(bitmap) = ICO.get(pixels..pixels + width * height * 4) else {
        return fallback();
    };
    let mut rgba = vec![0u8; width * height * 4];
    for row in 0..height {
        let source = &bitmap[(height - 1 - row) * width * 4..][..width * 4];
        let target = &mut rgba[row * width * 4..][..width * 4];
        for (pixel, out) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
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
    let options = eframe::NativeOptions {
        viewport: viewport_builder(startup.as_ref().ok()),
        ..Default::default()
    };
    eframe::run_native(
        "RMathTex",
        options,
        Box::new(move |creation| Ok(Box::new(app::RMathTexApp::new(creation, startup)?))),
    )
}

/// Applies the persisted window rectangle when the configuration was readable.
fn viewport_builder(config: Option<&config::Config>) -> egui::ViewportBuilder {
    let mut builder = egui::ViewportBuilder::default()
        .with_inner_size(model::WINDOW_SIZE)
        .with_min_inner_size(model::MIN_WINDOW_SIZE)
        .with_icon(window_icon());
    if let Some(window) = config.and_then(|config| config.window) {
        builder = builder
            .with_inner_size(window.size)
            .with_position(window.position)
            .with_maximized(window.maximized);
    }
    builder
}
