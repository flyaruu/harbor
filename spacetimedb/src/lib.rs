use spacetimedb::{ReducerContext, Table, Timestamp};

const MAX_LOCATION_REPORTS_PER_SHIP: usize = 10;

#[spacetimedb::table(accessor = ship, public)]
pub struct Ship {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::table(accessor = location_reporter, public)]
pub struct LocationReport {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    ship_id: u64,
    lat: f64,
    lon: f64,
    heading: f64,
    timestamp: Timestamp,
}

fn insert_location_report(
    ctx: &ReducerContext,
    ship_id: u64,
    lat: f64,
    lon: f64,
) -> Result<(), String> {
    if ctx.db.ship().id().find(&ship_id).is_none() {
        return Err(format!("Ship with id {ship_id} does not exist"));
    }

    let mut report_count = 0;
    let mut oldest_report_id: Option<u64> = None;

    for report in ctx.db.location_reporter().ship_id().filter(&ship_id) {
        report_count += 1;
        oldest_report_id = Some(match oldest_report_id {
            Some(oldest_id) => oldest_id.min(report.id),
            None => report.id,
        });
    }

    if report_count >= MAX_LOCATION_REPORTS_PER_SHIP {
        if let Some(oldest_report_id) = oldest_report_id {
            ctx.db.location_reporter().id().delete(&oldest_report_id);
        }
    }

    let row = ctx.db.location_reporter().insert(LocationReport {
        id: 0,
        ship_id,
        lat,
        lon,
        heading: 0.0,
        timestamp: ctx.timestamp,
    });
    log::info!("Created location report {} for ship {}", row.id, ship_id);

    Ok(())
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
pub fn add_ship(ctx: &ReducerContext, name: String) {
    let row = ctx.db.ship().insert(Ship { id: 0, name });

    log::info!("Created ship with id {}", row.id);
}

#[spacetimedb::reducer]
pub fn add_location_report(
    ctx: &ReducerContext,
    ship_id: u64,
    lat: f64,
    lon: f64,
) -> Result<(), String> {
    insert_location_report(ctx, ship_id, lat, lon)
}
