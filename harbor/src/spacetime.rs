use bevy::prelude::*;
use bevy_stdb::prelude::*;
use bevy_water::WaterSettings;
use spacetimedb_sdk::Table;
use spacetimedb_sdk::Timestamp;
use std::collections::HashSet;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::module_bindings::{
    CurrentShipProjection, CurrentShipProjectionTableAccess, DbConnection, LocationReport,
    LocationReportTableAccess, NewestLocationReportTimeTableAccess,
    OldestLocationReportTimeTableAccess, RemoteModule, Ship, ShipTableAccess,
    set_current_projection_request,
};
use crate::map::{MapRoot, TileWorldProjection};
use crate::ship::{PhysicalShip, ProjectedShip, spawn_projected_ship_pair};
use crate::ship_class::ShipClass;
use crate::ui::{
    CurrentTimestamp, TimestampBounds, TimestampUi, advance_timestamp_playback, format_timestamp,
};

const DEFAULT_SPACETIMEDB_URI: &str = "http://localhost:3000";
const DEFAULT_SPACETIMEDB_MODULE: &str = "ship-spacetime";
const NEWEST_LOCATION_REPORT_TIME_SQL: &str = "SELECT * FROM newest_location_report_time";
const OLDEST_LOCATION_REPORT_TIME_SQL: &str = "SELECT * FROM oldest_location_report_time";
const LOCATION_REPORT_SQL: &str = "SELECT * FROM location_report";
const SHIP_SQL: &str = "SELECT * FROM ship";
const CURRENT_SHIP_PROJECTION_SQL: &str = "SELECT * FROM current_ship_projection";

pub type StdbConn = StdbConnection<DbConnection>;
type StdbSubs = StdbSubscriptions<SubscriptionKey, RemoteModule>;

pub struct SpacetimePlugin;

#[derive(Default, Resource)]
struct ProjectionRefreshTiming {
    started_at: Option<Instant>,
    completed_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SubscriptionKey {
    NewestLocationReportTime,
    OldestLocationReportTime,
    LocationReport,
    Ship,
    CurrentShipProjection,
}

impl Plugin for SpacetimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectionRefreshTiming>()
            .add_plugins(stdb_plugin())
            .add_systems(
                Update,
                (
                    (
                        subscribe_on_connect,
                        sync_timestamp_bounds_from_cache,
                        extend_timestamp_bounds_from_live_reports,
                        sync_initial_timestamp_from_cache,
                        advance_timestamp_playback,
                        request_current_projection_on_timestamp_change,
                    )
                        .chain(),
                    reconcile_projected_ships_from_cache,
                    log_connection_errors,
                    log_disconnects,
                    log_subscription_events,
                ),
            );
    }
}

fn stdb_plugin() -> impl Plugin {
    #[cfg(target_arch = "wasm32")]
    let driver = DbConnection::run_background_task;
    #[cfg(not(target_arch = "wasm32"))]
    let driver = DbConnection::run_threaded;

    let mut plugin = StdbPlugin::<DbConnection, RemoteModule>::default()
        .with_eager_connection()
        .with_database_name(spacetimedb_module())
        .with_uri(spacetimedb_uri())
        .add_table::<LocationReport>(|reg, db| reg.bind(db.location_report()))
        .add_table::<Ship>(|reg, db| reg.bind(db.ship()))
        .with_subscriptions::<SubscriptionKey>()
        .with_reconnect(StdbReconnectOptions::default())
        .with_background_driver(driver);

    if let Some(token) = spacetimedb_token() {
        plugin = plugin.with_token(token);
    }

    plugin
}

fn subscribe_on_connect(
    mut connected: ReadStdbConnectedMessage,
    mut subscriptions: ResMut<StdbSubs>,
) {
    if connected.read().next().is_none() {
        return;
    }

    info!(
        "connected to SpacetimeDB module '{}' at {}",
        spacetimedb_module(),
        spacetimedb_uri()
    );

    subscriptions.subscribe_sql(
        SubscriptionKey::NewestLocationReportTime,
        NEWEST_LOCATION_REPORT_TIME_SQL,
    );
    subscriptions.subscribe_sql(
        SubscriptionKey::OldestLocationReportTime,
        OLDEST_LOCATION_REPORT_TIME_SQL,
    );
    subscriptions.subscribe_sql(SubscriptionKey::LocationReport, LOCATION_REPORT_SQL);
    subscriptions.subscribe_sql(SubscriptionKey::Ship, SHIP_SQL);
    subscriptions.subscribe_sql(
        SubscriptionKey::CurrentShipProjection,
        CURRENT_SHIP_PROJECTION_SQL,
    );
}

fn request_current_projection_on_timestamp_change(
    current_timestamp: Res<CurrentTimestamp>,
    connection: Option<Res<StdbConn>>,
    mut projection_timing: ResMut<ProjectionRefreshTiming>,
) {
    if !current_timestamp.is_changed() {
        return;
    }

    let Some(connection) = connection else {
        return;
    };

    let Some(current_timestamp) = current_timestamp.0.as_ref() else {
        return;
    };

    request_current_projection(&connection, current_timestamp, &mut projection_timing);
}

fn sync_initial_timestamp_from_cache(
    bounds: Res<TimestampBounds>,
    connection: Option<Res<StdbConn>>,
    mut timestamp_ui: ResMut<TimestampUi>,
    mut current_timestamp: ResMut<CurrentTimestamp>,
) {
    if current_timestamp.0.is_some() || !timestamp_ui.value.is_empty() {
        return;
    }

    if let Some(oldest_timestamp) = bounds.oldest {
        timestamp_ui.value = format_timestamp(oldest_timestamp);
        current_timestamp.0 = Some(oldest_timestamp);
        return;
    }

    let Some(connection) = connection else {
        return;
    };

    let Some(oldest) = connection.db().oldest_location_report_time().iter().next() else {
        return;
    };

    let Ok(oldest_timestamp) = oldest.timestamp.to_chrono_date_time() else {
        warn!("failed to convert oldest_location_report_time timestamp");
        return;
    };

    timestamp_ui.value = format_timestamp(oldest_timestamp);
    current_timestamp.0 = Some(oldest_timestamp);
}

fn sync_timestamp_bounds_from_cache(
    connection: Option<Res<StdbConn>>,
    mut bounds: ResMut<TimestampBounds>,
) {
    let Some(connection) = connection else {
        return;
    };

    let oldest = connection
        .db()
        .oldest_location_report_time()
        .iter()
        .next()
        .and_then(|row| row.timestamp.to_chrono_date_time().ok());
    let newest = connection
        .db()
        .newest_location_report_time()
        .iter()
        .next()
        .and_then(|row| row.timestamp.to_chrono_date_time().ok());

    // Subscription caches can populate oldest/newest at different times and can briefly drop one
    // side during delete/insert refreshes. Merge each side independently, but if a one-sided update
    // would invert the range, clear the stale opposite side instead of preserving a hybrid pair.
    let mut merged_oldest = oldest.or(bounds.oldest);
    let mut merged_newest = newest.or(bounds.newest);

    if let (Some(merged_oldest_value), Some(merged_newest_value)) = (merged_oldest, merged_newest)
        && merged_oldest_value > merged_newest_value
    {
        match (oldest.is_some(), newest.is_some()) {
            (true, false) => merged_newest = None,
            (false, true) => merged_oldest = None,
            _ => {
                merged_oldest = bounds.oldest;
                merged_newest = bounds.newest;
            }
        }
    }

    if bounds.oldest != merged_oldest || bounds.newest != merged_newest {
        bounds.oldest = merged_oldest;
        bounds.newest = merged_newest;
    }
}

fn extend_timestamp_bounds_from_live_reports(
    mut updates: ReadInsertUpdateMessage<LocationReport>,
    mut bounds: ResMut<TimestampBounds>,
) {
    let mut newest_seen = bounds.newest;
    let mut oldest_seen = bounds.oldest;

    for message in updates.read() {
        let Ok(timestamp) = message.new.timestamp.to_chrono_date_time() else {
            continue;
        };

        newest_seen = Some(newest_seen.map_or(timestamp, |current| current.max(timestamp)));
        oldest_seen = Some(oldest_seen.map_or(timestamp, |current| current.min(timestamp)));
    }

    if bounds.oldest != oldest_seen || bounds.newest != newest_seen {
        bounds.oldest = oldest_seen;
        bounds.newest = newest_seen;
    }
}

const PROJECTION_VISIBILITY_WINDOW_MICROS: i64 = 10 * 60 * 1_000_000;


fn request_current_projection(
    connection: &StdbConn,
    current_timestamp: &chrono::DateTime<chrono::Utc>,
    projection_timing: &mut ProjectionRefreshTiming,
) {
    let timestamp = Timestamp::parse_from_rfc3339(&current_timestamp.to_rfc3339())
        .expect("current timestamp should always format as RFC3339");

    if let Err(error) = connection
        .reducers()
        .set_current_projection_request(timestamp, PROJECTION_VISIBILITY_WINDOW_MICROS)
    {
        warn!("failed to request current ship projection: {error}");
    } else {
        projection_timing.started_at = Some(Instant::now());
    }
}

fn log_connection_errors(mut errors: ReadStdbConnectErrorMessage) {
    for message in errors.read() {
        error!("SpacetimeDB connection error: {}", message.err);
    }
}

fn log_disconnects(mut disconnects: ReadStdbDisconnectedMessage) {
    for message in disconnects.read() {
        match &message.err {
            Some(error) => warn!("SpacetimeDB disconnected: {error}"),
            None => warn!("SpacetimeDB disconnected"),
        }
    }
}

fn log_subscription_events(
    mut applied: ReadStdbSubscriptionAppliedMessage<SubscriptionKey>,
    mut errors: ReadStdbSubscriptionErrorMessage<SubscriptionKey>,
) {
    for message in applied.read() {
        info!(?message.key, "SpacetimeDB subscription applied");
    }

    for message in errors.read() {
        warn!(?message.key, error = %message.err, "SpacetimeDB subscription failed");
    }
}

fn reconcile_projected_ships_from_cache(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    projection: Res<TileWorldProjection>,
    water_settings: Res<WaterSettings>,
    map_root: Res<MapRoot>,
    connection: Option<Res<StdbConn>>,
    mut projection_timing: ResMut<ProjectionRefreshTiming>,
    mut projected_ships: Query<(Entity, &mut ProjectedShip)>,
    physical_ships: Query<(Entity, &PhysicalShip)>,
) {
    let Some(connection) = connection else {
        return;
    };

    let mut visible_ship_ids = HashSet::new();

    for projection_row in connection.db().current_ship_projection().iter() {
        visible_ship_ids.insert(projection_row.ship_mmsi);
        sync_projected_ship_entity(
            &mut commands,
            &asset_server,
            &projection,
            water_settings.height,
            &map_root,
            Some(&connection),
            &mut projected_ships,
            &physical_ships,
            &projection_row,
            false,
        );
    }

    for (entity, projected_ship) in &projected_ships {
        if visible_ship_ids.contains(&projected_ship.ship_id) {
            continue;
        }

        commands.entity(entity).despawn();

        for (physical_entity, physical_ship) in &physical_ships {
            if physical_ship.ship_id == projected_ship.ship_id {
                commands.entity(physical_entity).despawn();
            }
        }
    }

    if let Some(started_at) = projection_timing.started_at.take() {
        projection_timing.completed_count += 1;
        if projection_timing.completed_count % 10 == 0 {
            info!(
                refresh_count = projection_timing.completed_count,
                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                "current ship projection refresh completed"
            );
        }
    }
}

fn spacetimedb_uri() -> String {
    runtime_config_value("spacetimedb_uri", "SPACETIMEDB_URI")
        .unwrap_or_else(|| DEFAULT_SPACETIMEDB_URI.to_owned())
}

fn spacetimedb_module() -> String {
    runtime_config_value("spacetimedb_module", "SPACETIMEDB_MODULE")
        .unwrap_or_else(|| DEFAULT_SPACETIMEDB_MODULE.to_owned())
}

fn spacetimedb_token() -> Option<String> {
    runtime_config_value("spacetimedb_token", "SPACETIMEDB_TOKEN")
}

#[cfg(not(target_arch = "wasm32"))]
fn runtime_config_value(_browser_key: &str, env_key: &str) -> Option<String> {
    std::env::var(env_key).ok()
}

#[cfg(target_arch = "wasm32")]
fn runtime_config_value(browser_key: &str, _env_key: &str) -> Option<String> {
    use web_sys::{UrlSearchParams, window};

    let window = window()?;
    let search = window.location().search().ok()?;
    let params = UrlSearchParams::new_with_str(&search).ok()?;
    params.get(browser_key)
}

fn projected_ship_name(connection: Option<&StdbConn>, ship_id: u64) -> String {
    connection
        .and_then(|connection| connection.db().ship().mmsi().find(&ship_id))
        .map(|ship| ship.name)
        .unwrap_or_else(|| format!("Projected Ship {ship_id}"))
}

fn projected_ship_class(connection: Option<&StdbConn>, ship_id: u64) -> ShipClass {
    connection
        .and_then(|connection| connection.db().ship().mmsi().find(&ship_id))
        .as_ref()
        .map(|ship| {
            let determined_type= ShipClass::from_major_ais_type(ship.major_ship_type.as_ref());

            // info!("determining ship class for ship_id {ship_id} type: {:?} from original: {:?}", determined_type, ship.ship_type);
            determined_type
        })
        .unwrap_or(ShipClass::Default)

}

fn sync_projected_ship_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projection: &TileWorldProjection,
    water_height: f32,
    map_root: &MapRoot,
    connection: Option<&StdbConn>,
    projected_ships: &mut Query<(Entity, &mut ProjectedShip)>,
    physical_ships: &Query<(Entity, &PhysicalShip)>,
    current_projection: &CurrentShipProjection,
    log_changes: bool,
) {
    let ship_id = current_projection.ship_mmsi;
    let ship_name = projected_ship_name(connection, ship_id);
    let ship_class = projected_ship_class(connection, ship_id);
    let world_position = projection.lat_lon_to_world(current_projection.lat, current_projection.lon);

    let mut existing_entity = None;

    for (entity, mut projected_ship) in projected_ships.iter_mut() {
        if projected_ship.ship_id != ship_id {
            continue;
        }

        projected_ship.lat = current_projection.lat;
        projected_ship.lon = current_projection.lon;
        projected_ship.cog = current_projection.cog;
        projected_ship.sog = current_projection.sog;
        existing_entity = Some(entity);
        break;
    }

    if let Some(entity) = existing_entity {
        sync_physical_ship_name(commands, physical_ships, ship_id, &ship_name);
        commands.entity(entity).insert(Name::new(ship_name));
        return;
    }

    if log_changes {
        info!(
            ship_id,
            lat = current_projection.lat,
            lon = current_projection.lon,
            world_x = world_position.x,
            world_z = world_position.z,
            cog = ?current_projection.cog,
            sog = ?current_projection.sog,
            "spawning projected ship"
        );
    }

    spawn_projected_ship(
        commands,
        asset_server,
        projection,
        water_height,
        map_root,
        ship_class,
        ship_id,
        &ship_name,
        current_projection.lat,
        current_projection.lon,
        current_projection.cog,
        current_projection.sog,
    );
}

fn spawn_projected_ship(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projection: &TileWorldProjection,
    water_height: f32,
    map_root: &MapRoot,
    ship_class: ShipClass,
    ship_id: u64,
    ship_name: &str,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) {
    let _ = spawn_projected_ship_pair(
        commands,
        asset_server,
        projection,
        water_height,
        map_root,
        ship_class,
        ship_id,
        ship_name,
        lat,
        lon,
        cog,
        sog,
    );
}

fn sync_physical_ship_name(
    commands: &mut Commands,
    physical_ships: &Query<(Entity, &PhysicalShip)>,
    ship_id: u64,
    ship_name: &str,
) {
    for (entity, physical_ship) in physical_ships {
        if physical_ship.ship_id == ship_id {
            commands.entity(entity).insert(Name::new(ship_name.to_owned()));
            break;
        }
    }
}
