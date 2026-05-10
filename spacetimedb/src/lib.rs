use std::collections::BTreeMap;

use spacetimedb::{
    ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext, view,
};

mod mmsi_types;
use mmsi_types::MajorAisShipType;
#[spacetimedb::table(accessor = ship, public)]
pub struct Ship {
    #[primary_key]
    mmsi: u64,
    name: String,
    call_sign: Option<String>,
    destination: Option<String>,
    dimension_a: Option<u16>,
    dimension_b: Option<u16>,
    dimension_c: Option<u16>,
    dimension_d: Option<u16>,
    dte: Option<bool>,
    eta_month: Option<u8>,
    eta_day: Option<u8>,
    eta_hour: Option<u8>,
    eta_minute: Option<u8>,
    fix_type: Option<u8>,
    imo_number: Option<u32>,
    maximum_static_draught: Option<f64>,
    ship_type: Option<u8>,
    ais_version: Option<u8>,
    #[default(None::<MajorAisShipType>)]
    major_ship_type: Option<MajorAisShipType>,
}

#[spacetimedb::table(
    accessor = location_report,
    public,
    index(accessor = by_ship_and_time, btree(columns = [ship_mmsi, timestamp])),
    index(accessor = by_time, btree(columns = [timestamp]))
)]
pub struct LocationReport {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    ship_mmsi: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
    timestamp: Timestamp,
}

#[spacetimedb::table(accessor = ship_projection, public)]
pub struct ShipProjection {
    #[primary_key]
    ship_mmsi: u64,
    query_timestamp: Timestamp,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
    before_timestamp: Timestamp,
    after_timestamp: Option<Timestamp>,
    used_dead_reckoning: bool,
}

#[derive(Default)]
struct ReportWindow {
    before: Option<LocationReport>,
    after: Option<LocationReport>,
}

struct ProjectionEstimate {
    query_timestamp: Timestamp,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
    before_timestamp: Timestamp,
    after_timestamp: Option<Timestamp>,
    used_dead_reckoning: bool,
}

fn ship_projection_equal(left: &ShipProjection, right: &ShipProjection) -> bool {
    left.ship_mmsi == right.ship_mmsi
        && left.query_timestamp == right.query_timestamp
        && left.lat == right.lat
        && left.lon == right.lon
        && left.cog == right.cog
        && left.sog == right.sog
        && left.before_timestamp == right.before_timestamp
        && left.after_timestamp == right.after_timestamp
        && left.used_dead_reckoning == right.used_dead_reckoning
}

struct ShipStaticUpdate {
    name: String,
    call_sign: Option<String>,
    destination: Option<String>,
    dimension_a: Option<u16>,
    dimension_b: Option<u16>,
    dimension_c: Option<u16>,
    dimension_d: Option<u16>,
    dte: Option<bool>,
    eta_month: Option<u8>,
    eta_day: Option<u8>,
    eta_hour: Option<u8>,
    eta_minute: Option<u8>,
    fix_type: Option<u8>,
    imo_number: Option<u32>,
    maximum_static_draught: Option<f64>,
    ship_type: Option<u8>,
    major_ship_type: Option<MajorAisShipType>,
    ais_version: Option<u8>,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct OldestLocationReportTime {
    timestamp: Timestamp,
}

fn insert_location_report(
    ctx: &ReducerContext,
    ship_mmsi: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) -> Result<(), String> {
    // if ctx.db.ship().mmsi().find(&ship_mmsi).is_none() {
    //     return Err(format!("Ship with MMSI {ship_mmsi} does not exist"));
    // }

    let row = ctx.db.location_report().insert(LocationReport {
        id: 0,
        ship_mmsi,
        lat,
        lon,
        cog,
        sog,
        timestamp: ctx.timestamp,
    });
    let _ = row;

    Ok(())
}

fn merge_ship(existing: &Ship, incoming_name: String, incoming_call_sign: Option<String>) -> Ship {
    let name = if incoming_name.trim().is_empty() {
        existing.name.clone()
    } else {
        incoming_name
    };

    let call_sign = incoming_call_sign.or(existing.call_sign.clone());

    Ship {
        mmsi: existing.mmsi,
        name,
        call_sign,
        destination: existing.destination.clone(),
        dimension_a: existing.dimension_a,
        dimension_b: existing.dimension_b,
        dimension_c: existing.dimension_c,
        dimension_d: existing.dimension_d,
        dte: existing.dte,
        eta_month: existing.eta_month,
        eta_day: existing.eta_day,
        eta_hour: existing.eta_hour,
        eta_minute: existing.eta_minute,
        fix_type: existing.fix_type,
        imo_number: existing.imo_number,
        maximum_static_draught: existing.maximum_static_draught,
        ship_type: existing.ship_type,
        ais_version: existing.ais_version,
        major_ship_type: existing.major_ship_type,
    }
}

fn ships_equal(left: &Ship, right: &Ship) -> bool {
    left.mmsi == right.mmsi
        && left.name == right.name
        && left.call_sign == right.call_sign
        && left.destination == right.destination
        && left.dimension_a == right.dimension_a
        && left.dimension_b == right.dimension_b
        && left.dimension_c == right.dimension_c
        && left.dimension_d == right.dimension_d
        && left.dte == right.dte
        && left.eta_month == right.eta_month
        && left.eta_day == right.eta_day
        && left.eta_hour == right.eta_hour
        && left.eta_minute == right.eta_minute
        && left.fix_type == right.fix_type
        && left.imo_number == right.imo_number
        && left.maximum_static_draught == right.maximum_static_draught
        && left.ship_type == right.ship_type
        && left.ais_version == right.ais_version
        && left.major_ship_type == right.major_ship_type
}

fn normalize_optional_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_eta_month(month: u8) -> Option<u8> {
    (1..=12).contains(&month).then_some(month)
}

fn normalize_eta_day(day: u8) -> Option<u8> {
    (1..=31).contains(&day).then_some(day)
}

fn normalize_eta_hour(hour: u8) -> Option<u8> {
    (hour < 24).then_some(hour)
}

fn normalize_eta_minute(minute: u8) -> Option<u8> {
    (minute < 60).then_some(minute)
}

fn normalize_imo_number(imo_number: u32) -> Option<u32> {
    (imo_number != 0).then_some(imo_number)
}

fn normalize_static_update(
    name: String,
    call_sign: String,
    destination: String,
    dimension_a: u16,
    dimension_b: u16,
    dimension_c: u16,
    dimension_d: u16,
    dte: bool,
    eta_month: u8,
    eta_day: u8,
    eta_hour: u8,
    eta_minute: u8,
    fix_type: u8,
    imo_number: u32,
    maximum_static_draught: f64,
    ship_type: u8,
    ais_version: u8,
) -> ShipStaticUpdate {
    ShipStaticUpdate {
        name,
        call_sign: normalize_optional_string(call_sign),
        destination: normalize_optional_string(destination),
        dimension_a: Some(dimension_a),
        dimension_b: Some(dimension_b),
        dimension_c: Some(dimension_c),
        dimension_d: Some(dimension_d),
        dte: Some(dte),
        eta_month: normalize_eta_month(eta_month),
        eta_day: normalize_eta_day(eta_day),
        eta_hour: normalize_eta_hour(eta_hour),
        eta_minute: normalize_eta_minute(eta_minute),
        fix_type: Some(fix_type),
        imo_number: normalize_imo_number(imo_number),
        maximum_static_draught: Some(maximum_static_draught),
        ship_type: Some(ship_type),
        ais_version: Some(ais_version),
        major_ship_type: Some(MajorAisShipType::from(ship_type)),
    }
}

fn merge_ship_static_data(existing: Option<&Ship>, mmsi: u64, update: ShipStaticUpdate) -> Ship {
    let ShipStaticUpdate {
        name,
        call_sign,
        destination,
        dimension_a,
        dimension_b,
        dimension_c,
        dimension_d,
        dte,
        eta_month,
        eta_day,
        eta_hour,
        eta_minute,
        fix_type,
        imo_number,
        maximum_static_draught,
        ship_type,
        ais_version,
        major_ship_type,
    } = update;

    if let Some(existing) = existing {
        let merged = merge_ship(existing, name, call_sign);
        Ship {
            mmsi,
            name: merged.name,
            call_sign: merged.call_sign,
            destination: destination.or(merged.destination),
            dimension_a: dimension_a.or(merged.dimension_a),
            dimension_b: dimension_b.or(merged.dimension_b),
            dimension_c: dimension_c.or(merged.dimension_c),
            dimension_d: dimension_d.or(merged.dimension_d),
            dte: dte.or(merged.dte),
            eta_month: eta_month.or(merged.eta_month),
            eta_day: eta_day.or(merged.eta_day),
            eta_hour: eta_hour.or(merged.eta_hour),
            eta_minute: eta_minute.or(merged.eta_minute),
            fix_type: fix_type.or(merged.fix_type),
            imo_number: imo_number.or(merged.imo_number),
            maximum_static_draught: maximum_static_draught.or(merged.maximum_static_draught),
            ship_type: ship_type.or(merged.ship_type),
            ais_version: ais_version.or(merged.ais_version),
            major_ship_type: major_ship_type.or(merged.major_ship_type),
        }
    } else {
        Ship {
            mmsi,
            name: if name.trim().is_empty() {
                mmsi.to_string()
            } else {
                name
            },
            call_sign,
            destination,
            dimension_a,
            dimension_b,
            dimension_c,
            dimension_d,
            dte,
            eta_month,
            eta_day,
            eta_hour,
            eta_minute,
            fix_type,
            imo_number,
            maximum_static_draught,
            ship_type,
            ais_version,
            major_ship_type,
        }
    }
}

fn interpolate_location(
    before: &LocationReport,
    after: &LocationReport,
    query_timestamp: Timestamp,
) -> Option<(f64, f64)> {
    let fraction = interpolation_fraction(before.timestamp, after.timestamp, query_timestamp)?;

    Some((
        before.lat + (after.lat - before.lat) * fraction,
        before.lon + (after.lon - before.lon) * fraction,
    ))
}

fn interpolation_fraction(
    before_timestamp: Timestamp,
    after_timestamp: Timestamp,
    query_timestamp: Timestamp,
) -> Option<f64> {
    let total_micros = after_timestamp
        .to_micros_since_unix_epoch()
        .checked_sub(before_timestamp.to_micros_since_unix_epoch())?;

    if total_micros <= 0 {
        return Some(0.0);
    }

    let elapsed_micros = query_timestamp
        .to_micros_since_unix_epoch()
        .checked_sub(before_timestamp.to_micros_since_unix_epoch())?;

    Some((elapsed_micros as f64 / total_micros as f64).clamp(0.0, 1.0))
}

fn interpolate_optional_value(
    before: Option<f64>,
    after: Option<f64>,
    fraction: f64,
) -> Option<f64> {
    match (before, after) {
        (Some(before), Some(after)) => Some(before + (after - before) * fraction),
        (Some(before), None) => Some(before),
        (None, Some(after)) => Some(after),
        (None, None) => None,
    }
}

fn dead_reckon_location(report: &LocationReport, query_timestamp: Timestamp) -> Option<(f64, f64)> {
    let sog = report.sog?;
    let cog = report.cog?;

    let elapsed = query_timestamp.duration_since(report.timestamp)?;
    let elapsed_hours = elapsed.as_secs_f64() / 3600.0;
    let distance_nm = sog * elapsed_hours;
    let bearing_radians = cog.to_radians();

    let delta_lat = distance_nm * bearing_radians.cos() / 60.0;
    let latitude_radians = report.lat.to_radians();
    let cos_lat = latitude_radians.cos();

    let delta_lon = if cos_lat.abs() < f64::EPSILON {
        0.0
    } else {
        distance_nm * bearing_radians.sin() / (60.0 * cos_lat)
    };

    Some((report.lat + delta_lat, report.lon + delta_lon))
}

fn estimate_projection(
    window: &ReportWindow,
    query_timestamp: Timestamp,
) -> Option<ProjectionEstimate> {
    match (&window.before, &window.after) {
        (Some(before), Some(after)) => {
            let (lat, lon) = interpolate_location(before, after, query_timestamp)?;
            let fraction =
                interpolation_fraction(before.timestamp, after.timestamp, query_timestamp)?;
            let cog = interpolate_optional_value(before.cog, after.cog, fraction);
            let sog = interpolate_optional_value(before.sog, after.sog, fraction);

            Some(ProjectionEstimate {
                query_timestamp,
                lat,
                lon,
                cog,
                sog,
                before_timestamp: before.timestamp,
                after_timestamp: Some(after.timestamp),
                used_dead_reckoning: false,
            })
        }
        (Some(before), None) => {
            let (lat, lon) =
                dead_reckon_location(before, query_timestamp).unwrap_or((before.lat, before.lon));

            Some(ProjectionEstimate {
                query_timestamp,
                lat,
                lon,
                cog: before.cog,
                sog: before.sog,
                before_timestamp: before.timestamp,
                after_timestamp: None,
                used_dead_reckoning: before.sog.is_some() && before.cog.is_some(),
            })
        }
        (None, Some(_)) | (None, None) => None,
    }
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb::reducer]
pub fn add_ship(
    ctx: &ReducerContext,
    name: String,
    call_sign: Option<String>,
    mmsi: u64,
) -> Result<(), String> {
    if let Some(existing) = ctx.db.ship().mmsi().find(&mmsi) {
        let merged = merge_ship(&existing, name, call_sign);
        if !ships_equal(&existing, &merged) {
            ctx.db.ship().mmsi().update(merged);
        }
    } else {
        ctx.db.ship().insert(Ship {
            mmsi,
            name,
            call_sign,
            destination: None,
            dimension_a: None,
            dimension_b: None,
            dimension_c: None,
            dimension_d: None,
            dte: None,
            eta_month: None,
            eta_day: None,
            eta_hour: None,
            eta_minute: None,
            fix_type: None,
            imo_number: None,
            maximum_static_draught: None,
            ship_type: None,
            ais_version: None,
            major_ship_type: None,
        });
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn upsert_ship_static_data(
    ctx: &ReducerContext,
    mmsi: u64,
    name: String,
    call_sign: String,
    destination: String,
    dimension_a: u16,
    dimension_b: u16,
    dimension_c: u16,
    dimension_d: u16,
    dte: bool,
    eta_month: u8,
    eta_day: u8,
    eta_hour: u8,
    eta_minute: u8,
    fix_type: u8,
    imo_number: u32,
    maximum_static_draught: f64,
    ship_type: u8,
    ais_version: u8,
) -> Result<(), String> {
    let update = normalize_static_update(
        name,
        call_sign,
        destination,
        dimension_a,
        dimension_b,
        dimension_c,
        dimension_d,
        dte,
        eta_month,
        eta_day,
        eta_hour,
        eta_minute,
        fix_type,
        imo_number,
        maximum_static_draught,
        ship_type,
        ais_version,
    );
    let existing = ctx.db.ship().mmsi().find(&mmsi);
    let next_row = merge_ship_static_data(existing.as_ref(), mmsi, update);

    if let Some(existing) = existing {
        if !ships_equal(&existing, &next_row) {
            ctx.db.ship().mmsi().update(next_row);
        }
    } else {
        ctx.db.ship().insert(next_row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn backfill_major_ship_types(ctx: &ReducerContext) -> Result<(), String> {
    for ship in ctx.db.ship().iter() {
        if ship.major_ship_type.is_some() {
            continue;
        }

        let Some(ship_type) = ship.ship_type else {
            continue;
        };

        ctx.db.ship().mmsi().update(Ship {
            major_ship_type: Some(MajorAisShipType::from(ship_type)),
            ..ship
        });
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn add_location_report(
    ctx: &ReducerContext,
    ship_mmsi: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) -> Result<(), String> {
    insert_location_report(ctx, ship_mmsi, lat, lon, cog, sog)
}

// const

#[spacetimedb::reducer]
pub fn set_current_time(_ctx: &ReducerContext, timestamp: Timestamp) -> Result<(), String> {
    let _time2 = timestamp
        .checked_add(TimeDuration::from_micros(60_000_000))
        .ok_or("Timestamp overflow")?;
    Ok(())
}

#[view(accessor = oldest_location_report_time, public)]
pub fn oldest_location_report_time(ctx: &ViewContext) -> Option<OldestLocationReportTime> {
    let report = ctx
        .db
        .location_report()
        .by_time()
        .filter(Timestamp::UNIX_EPOCH..)
        .next()?;

    Some(OldestLocationReportTime {
        timestamp: report.timestamp,
    })
}

// Note that it isn't very efficient to scan through all reports to find the newest timestamp, but this is just an example.
#[view(accessor = newest_location_report_time, public)]
pub fn newest_location_report_time(ctx: &ViewContext) -> Option<OldestLocationReportTime> {
    let mut newest = None;
    for report in ctx
        .db
        .location_report()
        .by_time()
        .filter(Timestamp::UNIX_EPOCH..)
    {
        newest = Some(report.timestamp);
    }

    Some(OldestLocationReportTime { timestamp: newest? })
}

#[spacetimedb::reducer]
pub fn project_ship_locations(
    ctx: &ReducerContext,
    query_timestamp: Timestamp,
    visibility_window_micros: i64,
) -> Result<(), String> {
    if visibility_window_micros <= 0 {
        return Err("visibility_window_micros must be greater than 0".to_string());
    }

    let mut windows: BTreeMap<u64, ReportWindow> = BTreeMap::new();
    let projection_visibility_window = TimeDuration::from_micros(visibility_window_micros);
    let window_start = query_timestamp
        .checked_sub(projection_visibility_window)
        .ok_or("Projection window underflow")?;
    let window_end_exclusive = query_timestamp
        .checked_add(projection_visibility_window)
        .ok_or("Projection window overflow")?
        .checked_add(TimeDuration::from_micros(1))
        .ok_or("Projection window overflow")?;

    for report in ctx
        .db
        .location_report()
        .by_time()
        .filter(window_start..window_end_exclusive)
    {
        let window = windows.entry(report.ship_mmsi).or_default();

        if report.timestamp <= query_timestamp {
            let replace_before = window
                .before
                .as_ref()
                .map(|existing| report.timestamp > existing.timestamp)
                .unwrap_or(true);

            if replace_before {
                window.before = Some(report);
            }
        } else {
            let replace_after = window
                .after
                .as_ref()
                .map(|existing| report.timestamp < existing.timestamp)
                .unwrap_or(true);

            if replace_after {
                window.after = Some(report);
            }
        }
    }

    let mut projections: BTreeMap<u64, ProjectionEstimate> = BTreeMap::new();

    for (ship_mmsi, window) in windows {
        if let Some(projection) = estimate_projection(&window, query_timestamp) {
            projections.insert(ship_mmsi, projection);
        }
    }

    let existing_projections: BTreeMap<u64, ShipProjection> = ctx
        .db
        .ship_projection()
        .iter()
        .map(|projection| (projection.ship_mmsi, projection))
        .collect();

    for ship_mmsi in existing_projections.keys() {
        if !projections.contains_key(ship_mmsi) {
            ctx.db.ship_projection().ship_mmsi().delete(ship_mmsi);
        }
    }

    for (ship_mmsi, projection) in projections {
        let next_row = ShipProjection {
            ship_mmsi,
            query_timestamp: projection.query_timestamp,
            lat: projection.lat,
            lon: projection.lon,
            cog: projection.cog,
            sog: projection.sog,
            before_timestamp: projection.before_timestamp,
            after_timestamp: projection.after_timestamp,
            used_dead_reckoning: projection.used_dead_reckoning,
        };

        if let Some(existing) = existing_projections.get(&ship_mmsi) {
            if !ship_projection_equal(existing, &next_row) {
                ctx.db.ship_projection().ship_mmsi().update(next_row);
            }
        } else {
            ctx.db.ship_projection().insert(next_row);
        }
    }

    Ok(())
}
