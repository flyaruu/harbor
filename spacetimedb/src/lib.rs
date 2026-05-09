use std::collections::BTreeMap;

use spacetimedb::{ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext, view};

const PROJECTION_VISIBILITY_WINDOW_MICROS: i64 = 10 * 60 * 1_000_000;

#[spacetimedb::table(accessor = ship, public)]
pub struct Ship {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    call_sign: Option<String>,
}

#[spacetimedb::table(
    accessor = location_report,
    public,
    index(accessor = by_ship_and_time, btree(columns = [ship_id, timestamp])),
    index(accessor = by_time, btree(columns = [timestamp]))
)]
pub struct LocationReport {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    ship_id: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
    timestamp: Timestamp,
}

#[spacetimedb::table(accessor = ship_projection, public)]
pub struct ShipProjection {
    #[primary_key]
    ship_id: u64,
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

#[derive(SpacetimeType, Clone, Debug)]
pub struct OldestLocationReportTime {
    timestamp: Timestamp,
}

fn insert_location_report(
    ctx: &ReducerContext,
    ship_id: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) -> Result<(), String> {
    // if ctx.db.ship().id().find(&ship_id).is_none() {
    //     return Err(format!("Ship with id {ship_id} does not exist"));
    // }

    let row = ctx.db.location_report().insert(LocationReport {
        id: 0,
        ship_id,
        lat,
        lon,
        cog,
        sog,
        timestamp: ctx.timestamp,
    });
    log::info!("Created location report {} for ship {}", row.id, ship_id);

    Ok(())
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
) -> Result<(), String> {
    let row = ctx.db.ship().insert(Ship {
        id: 0,
        name,
        call_sign,
    });

    log::info!("Created ship with id {}", row.id);
    Ok(())
}

#[spacetimedb::reducer]
pub fn add_location_report(
    ctx: &ReducerContext,
    ship_id: u64,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) -> Result<(), String> {
    insert_location_report(ctx, ship_id, lat, lon, cog, sog)
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

    Some(OldestLocationReportTime {
        timestamp: newest?,
    })
}

#[spacetimedb::reducer]
pub fn project_ship_locations(
    ctx: &ReducerContext,
    query_timestamp: Timestamp,
) -> Result<(), String> {
    let mut windows: BTreeMap<u64, ReportWindow> = BTreeMap::new();
    let projection_visibility_window =
        TimeDuration::from_micros(PROJECTION_VISIBILITY_WINDOW_MICROS);
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
        let window = windows.entry(report.ship_id).or_default();

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

    for (ship_id, window) in windows {
        if let Some(projection) = estimate_projection(&window, query_timestamp) {
            projections.insert(ship_id, projection);
        }
    }

    for existing in ctx.db.ship_projection().iter() {
        if !projections.contains_key(&existing.ship_id) {
            ctx.db.ship_projection().ship_id().delete(&existing.ship_id);
        }
    }

    for (ship_id, projection) in projections {
        let next_row = ShipProjection {
            ship_id,
            query_timestamp: projection.query_timestamp,
            lat: projection.lat,
            lon: projection.lon,
            cog: projection.cog,
            sog: projection.sog,
            before_timestamp: projection.before_timestamp,
            after_timestamp: projection.after_timestamp,
            used_dead_reckoning: projection.used_dead_reckoning,
        };

        if ctx.db.ship_projection().ship_id().find(&ship_id).is_some() {
            ctx.db.ship_projection().ship_id().update(next_row);
        } else {
            ctx.db.ship_projection().insert(next_row);
        }
    }

    Ok(())
}
