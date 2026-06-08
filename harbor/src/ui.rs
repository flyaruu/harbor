use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use chrono::{DateTime, SecondsFormat, Utc};

use crate::module_bindings::MajorAisShipType;

const PLAYBACK_RATE_NORMAL: f32 = 1.0;
const PLAYBACK_RATE_FAST: f32 = 10.0;
const PLAYBACK_RATE_VERY_FAST: f32 = 100.0;

#[derive(Default, Resource)]
pub struct TimestampUi {
    pub value: String,
    pub is_editing: bool,
}

#[derive(Default, Resource)]
pub struct CurrentTimestamp(pub Option<DateTime<Utc>>);

#[derive(Default, Resource)]
pub struct TimestampBounds {
    pub oldest: Option<DateTime<Utc>>,
    pub newest: Option<DateTime<Utc>>,
}

#[derive(Default, Resource)]
pub struct TimestampPlayback {
    pub rate: f32,
}

#[derive(Resource)]
pub struct ShipInfoOverlay {
    pub ship_id: Option<u64>,
    pub call_sign: Option<String>,
    pub destination: Option<String>,
    pub ship_type: Option<MajorAisShipType>,
    pub dimension_a: Option<u16>,
    pub dimension_b: Option<u16>,
    pub dimension_c: Option<u16>,
    pub dimension_d: Option<u16>,
    pub name: String,
    pub course_over_ground: Option<f64>,
    pub speed_over_ground: Option<f64>,
    pub latitude: f64,
    pub longitude: f64,
    pub last_location_report_timestamp: Option<DateTime<Utc>>,
}

impl Default for ShipInfoOverlay {
    fn default() -> Self {
        Self {
            ship_id: Some(447_932_100),
            call_sign: Some("PHND".to_owned()),
            destination: None,
            ship_type: Some(MajorAisShipType::Cargo),
            dimension_a: None,
            dimension_b: None,
            dimension_c: None,
            dimension_d: None,
            name: "MV Horizon".to_owned(),
            course_over_ground: Some(84.2),
            speed_over_ground: Some(12.6),
            latitude: 51.9060,
            longitude: 4.4844,
            last_location_report_timestamp: None,
        }
    }
}

pub fn timestamp_ui(
    mut contexts: EguiContexts,
    mut timestamp: ResMut<TimestampUi>,
    bounds: Res<TimestampBounds>,
    ship_info: Res<ShipInfoOverlay>,
    mut playback: ResMut<TimestampPlayback>,
    mut current_timestamp: ResMut<CurrentTimestamp>,
) {
    let ctx = contexts.ctx_mut().expect("primary egui context");
    let mut timestamp_changed = false;
    let current_slider_timestamp = current_timestamp
        .0
        .or_else(|| parse_timestamp(&timestamp.value));

    if !timestamp.is_editing
        && let Some(current) = current_timestamp.0
    {
        let formatted = format_timestamp(current);
        if timestamp.value != formatted {
            timestamp.value = formatted;
        }
    }

    let decrement_pressed = if ctx.wants_keyboard_input() {
        false
    } else {
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Minus))
    };
    let increment_pressed = if ctx.wants_keyboard_input() {
        false
    } else {
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Equals))
    };

    if decrement_pressed {
        timestamp.is_editing = false;
        shift_timestamp(&mut timestamp.value, -1);
        timestamp_changed = true;
    }

    if increment_pressed {
        timestamp.is_editing = false;
        shift_timestamp(&mut timestamp.value, 1);
        timestamp_changed = true;
    }

    egui::Area::new("timestamp_controls".into())
        .fixed_pos(egui::pos2(12.0, 12.0))
        .interactable(true)
        .show(ctx, |ui| {
            overlay_frame().show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                    if ui.button("Play").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = PLAYBACK_RATE_NORMAL;
                    }

                    if ui.button("10x").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = PLAYBACK_RATE_FAST;
                    }

                    if ui.button("100x").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = PLAYBACK_RATE_VERY_FAST;
                    }

                    if ui.button("Stop").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = 0.0;
                    }

                    if ui.button("Reset").clicked()
                        && let Some(oldest_timestamp) = bounds.oldest
                    {
                        timestamp.is_editing = false;
                        playback.rate = 0.0;
                        timestamp.value = format_timestamp(oldest_timestamp);
                        timestamp_changed = true;
                    }

                    if ui.button("-").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = 0.0;
                        shift_timestamp(&mut timestamp.value, -1);
                        timestamp_changed = true;
                    }

                    let timestamp_input = ui
                        .add(egui::TextEdit::singleline(&mut timestamp.value).desired_width(220.0));

                    if timestamp_input.has_focus() {
                        timestamp.is_editing = true;
                    } else if timestamp_input.lost_focus() {
                        timestamp.is_editing = false;
                    }

                    if timestamp_input.changed() && timestamp.is_editing {
                        playback.rate = 0.0;
                        timestamp_changed = true;
                    }

                    if ui.button("+").clicked() {
                        timestamp.is_editing = false;
                        playback.rate = 0.0;
                        shift_timestamp(&mut timestamp.value, 1);
                        timestamp_changed = true;
                    }
                    });

                    if let Some((oldest, newest)) = slider_bounds(&bounds, current_slider_timestamp)
                    {
                        let mut slider_value = parse_timestamp(&timestamp.value)
                            .map(|value| value.timestamp())
                            .unwrap_or(oldest)
                            .clamp(oldest, newest);

                        if ui
                            .add_sized(
                                [ui.available_width().max(220.0), ui.spacing().interact_size.y],
                                egui::Slider::new(&mut slider_value, oldest..=newest)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            timestamp.is_editing = false;
                            playback.rate = 0.0;
                            let Some(slider_timestamp) = DateTime::from_timestamp(slider_value, 0)
                            else {
                                return;
                            };
                            timestamp.value = format_timestamp(slider_timestamp);
                            timestamp_changed = true;
                        }
                    }
                });
            });
        });

    egui::Area::new("ship_info_static_overlay".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .interactable(true)
        .show(ctx, |ui| {
            overlay_frame().show(ui, |ui| {
                ui.set_min_width(260.0);
                ui.heading("Ship Info");
                ui.separator();
                info_row(ui, "Ship ID", &format_optional_u64(ship_info.ship_id));
                info_row(ui, "Name", &ship_info.name);
                info_row(
                    ui,
                    "Call Sign",
                    &format_optional_text(ship_info.call_sign.as_deref()),
                );
                info_row(ui, "Type", &format_ship_type(ship_info.ship_type.as_ref()));
                info_row(ui, "AIS Dims", &format_ais_dimensions(&ship_info));
            });
        });

    egui::Area::new("ship_info_dynamic_overlay".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 208.0))
        .interactable(true)
        .show(ctx, |ui| {
            overlay_frame().show(ui, |ui| {
                ui.set_min_width(260.0);
                ui.heading("Live Data");
                ui.separator();
                info_row(
                    ui,
                    "Destination",
                    &format_optional_text(ship_info.destination.as_deref()),
                );
                info_row(
                    ui,
                    "COG",
                    &format_optional_f64(ship_info.course_over_ground, "deg"),
                );
                info_row(
                    ui,
                    "SOG",
                    &format_optional_f64(ship_info.speed_over_ground, "kn"),
                );
                info_row(
                    ui,
                    "Last Report",
                    &format_optional_timestamp(ship_info.last_location_report_timestamp),
                );
                info_row(ui, "Lat", &format!("{:.4}", ship_info.latitude));
                info_row(ui, "Lon", &format!("{:.4}", ship_info.longitude));
            });
        });

    if timestamp_changed && let Some(parsed) = parse_timestamp(&timestamp.value) {
        if current_timestamp.0.as_ref() != Some(&parsed) {
            current_timestamp.0 = Some(parsed);
        }
    }
}

pub fn advance_timestamp_playback(
    time: Res<Time>,
    bounds: Res<TimestampBounds>,
    mut playback: ResMut<TimestampPlayback>,
    mut timestamp_ui: ResMut<TimestampUi>,
    mut current_timestamp: ResMut<CurrentTimestamp>,
    mut last_step_elapsed_seconds: Local<Option<f64>>,
) {
    if playback.rate <= 0.0 {
        *last_step_elapsed_seconds = Some(time.elapsed_secs_f64());
        return;
    }

    let now = time.elapsed_secs_f64();
    let Some(previous) = last_step_elapsed_seconds.replace(now) else {
        return;
    };

    let elapsed_seconds = (now - previous).max(0.0) as f32;

    if elapsed_seconds <= f32::EPSILON {
        return;
    }

    let Some(current) = current_timestamp
        .0
        .or_else(|| parse_timestamp(&timestamp_ui.value))
    else {
        return;
    };

    let delta = chrono::TimeDelta::from_std(std::time::Duration::from_secs_f32(
        elapsed_seconds * playback.rate,
    ))
    .expect("positive playback delta should convert to chrono duration");
    let mut next = current + delta;

    if let Some(newest) = bounds.newest
        && next >= newest
    {
        next = newest;
        playback.rate = 0.0;
    }

    if let Some(oldest) = bounds.oldest
        && next < oldest
    {
        next = oldest;
    }

    if current_timestamp.0.as_ref() != Some(&next) {
        current_timestamp.0 = Some(next);
    }

    let formatted = format_timestamp(next);
    if timestamp_ui.value != formatted {
        timestamp_ui.value = formatted;
    }
}

pub fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn overlay_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_premultiplied(16, 20, 26, 220))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(8))
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, 0.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .strong()
                    .color(egui::Color32::from_rgb(210, 218, 228)),
            ),
        );
        ui.label(egui::RichText::new(value).color(egui::Color32::from_rgb(245, 248, 252)));
    });
}

fn format_optional_text(value: Option<&str>) -> String {
    value.unwrap_or("-").to_owned()
}

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn format_optional_f64(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value:.1} {unit}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn format_optional_timestamp(value: Option<DateTime<Utc>>) -> String {
    value
        .map(format_timestamp)
        .unwrap_or_else(|| "-".to_owned())
}

fn format_ais_dimensions(ship_info: &ShipInfoOverlay) -> String {
    match (
        ship_info.dimension_a,
        ship_info.dimension_b,
        ship_info.dimension_c,
        ship_info.dimension_d,
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => format!("a={a} b={b} c={c} d={d}"),
        _ => "-".to_owned(),
    }
}

fn format_ship_type(value: Option<&MajorAisShipType>) -> String {
    match value {
        Some(MajorAisShipType::NotAvailable) => "Not Available".to_owned(),
        Some(MajorAisShipType::Reserved) => "Reserved".to_owned(),
        Some(MajorAisShipType::WingInGround) => "Wing In Ground".to_owned(),
        Some(MajorAisShipType::Fishing) => "Fishing".to_owned(),
        Some(MajorAisShipType::Towing) => "Towing".to_owned(),
        Some(MajorAisShipType::TowingLarge) => "Towing Large".to_owned(),
        Some(MajorAisShipType::DredgingOrUnderwaterOps) => "Dredging or Underwater Ops".to_owned(),
        Some(MajorAisShipType::DivingOps) => "Diving Ops".to_owned(),
        Some(MajorAisShipType::MilitaryOps) => "Military Ops".to_owned(),
        Some(MajorAisShipType::Sailing) => "Sailing".to_owned(),
        Some(MajorAisShipType::PleasureCraft) => "Pleasure Craft".to_owned(),
        Some(MajorAisShipType::HighSpeedCraft) => "High Speed Craft".to_owned(),
        Some(MajorAisShipType::PilotVessel) => "Pilot Vessel".to_owned(),
        Some(MajorAisShipType::SearchAndRescueVessel) => "Search and Rescue Vessel".to_owned(),
        Some(MajorAisShipType::Tug) => "Tug".to_owned(),
        Some(MajorAisShipType::PortTender) => "Port Tender".to_owned(),
        Some(MajorAisShipType::AntiPollutionEquipment) => "Anti-Pollution Equipment".to_owned(),
        Some(MajorAisShipType::LawEnforcement) => "Law Enforcement".to_owned(),
        Some(MajorAisShipType::MedicalTransport) => "Medical Transport".to_owned(),
        Some(MajorAisShipType::NoncombatantShip) => "Noncombatant Ship".to_owned(),
        Some(MajorAisShipType::Passenger) => "Passenger".to_owned(),
        Some(MajorAisShipType::Cargo) => "Cargo".to_owned(),
        Some(MajorAisShipType::Tanker) => "Tanker".to_owned(),
        Some(MajorAisShipType::Other) => "Other".to_owned(),
        Some(MajorAisShipType::Unknown(code)) => format!("Unknown ({code})"),
        None => "-".to_owned(),
    }
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn slider_bounds(
    bounds: &TimestampBounds,
    current_timestamp: Option<DateTime<Utc>>,
) -> Option<(i64, i64)> {
    let oldest = bounds.oldest.map(|value| value.timestamp());
    let newest = bounds.newest.map(|value| value.timestamp());
    let current = current_timestamp.map(|value| value.timestamp());

    match (oldest, newest, current) {
        (Some(oldest), Some(newest), _) if oldest <= newest => Some((oldest, newest)),
        (Some(oldest), None, Some(current)) => Some((oldest.min(current), oldest.max(current))),
        (None, Some(newest), Some(current)) => Some((current.min(newest), current.max(newest))),
        (None, None, Some(current)) => Some((current, current)),
        (Some(oldest), None, None) => Some((oldest, oldest)),
        (None, Some(newest), None) => Some((newest, newest)),
        _ => None,
    }
}

fn shift_timestamp(value: &mut String, delta_seconds: i64) {
    let Some(parsed) = parse_timestamp(value) else {
        return;
    };

    let shifted = parsed + chrono::TimeDelta::seconds(delta_seconds);
    *value = format_timestamp(shifted);
}
