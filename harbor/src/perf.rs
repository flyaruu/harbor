use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub struct PerformancePlugin;

impl Plugin for PerformancePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(EguiPrimaryContextPass, fps_overlay_ui)
            .add_systems(Startup, log_profiling_status);
    }
}

fn fps_overlay_ui(diagnostics: Res<DiagnosticsStore>, mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut().expect("primary egui context");
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed());
    let frame_time_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.smoothed());

    egui::Area::new("performance_overlay".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
        .interactable(false)
        .show(ctx, |ui| {
            overlay_frame().show(ui, |ui| {
                if let Some(fps) = fps {
                    ui.label(
                        egui::RichText::new(format!("{fps:>5.1} fps"))
                            .monospace()
                            .color(egui::Color32::from_rgb(245, 248, 252)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("  n/a fps")
                            .monospace()
                            .color(egui::Color32::from_rgb(180, 188, 198)),
                    );
                }

                if let Some(frame_time_ms) = frame_time_ms {
                    ui.label(
                        egui::RichText::new(format!("{frame_time_ms:>5.2} ms"))
                            .monospace()
                            .color(egui::Color32::from_rgb(180, 188, 198)),
                    );
                }
            });
        });
}

fn overlay_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_premultiplied(16, 20, 26, 220))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(8))
}

#[cfg(feature = "chrome_trace")]
fn log_profiling_status() {
    info!(
        "Chrome trace profiling enabled. Inspect the generated trace with Perfetto or chrome://tracing for per-frame and per-system timings."
    );
}

#[cfg(not(feature = "chrome_trace"))]
fn log_profiling_status() {
    info!(
        "Frame diagnostics enabled. Re-run with `cargo run -p harbor --features chrome_trace` for per-system frame breakdowns in a Chrome trace."
    );
}
