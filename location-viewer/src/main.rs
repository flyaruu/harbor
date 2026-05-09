mod module_bindings;
use module_bindings::*;
use std::env;
use std::time::Duration;

use spacetimedb_sdk::{DbContext, Table, TableWithPrimaryKey, Timestamp};

const PROJECTION_TIMESTAMP_RFC3339: &str = "2026-05-08T11:08:29Z";

fn print_ship_projection(ctx: &EventContext, projection: &ShipProjection) {
    let ship_name = ctx
        .db()
        .ship()
        .id()
        .find(&projection.ship_id)
        .map(|ship| ship.name)
        .unwrap_or_else(|| format!("ship-{}", projection.ship_id));

    println!(
        "{} projected lat={}, lon={} dead_reckoning={}",
        ship_name, projection.lat, projection.lon, projection.used_dead_reckoning
    );
}

fn projection_timestamp() -> Timestamp {
    Timestamp::parse_from_rfc3339(PROJECTION_TIMESTAMP_RFC3339)
        .expect("Invalid hardcoded projection timestamp")
}

fn main() {
    let host: String = env::var("SPACETIMEDB_HOST").unwrap_or("http://localhost:3000".to_string());
    let db_name: String = env::var("SPACETIMEDB_DB_NAME").unwrap_or("my-db".to_string());
    eprintln!(
        "Connecting to SpacetimeDB at: {} with database: {}",
        host, db_name
    );

    let conn = DbConnection::builder()
        .with_database_name(db_name)
        .with_uri(host)
        .on_connect(|conn, _, _| {
            println!("Connected to SpacetimeDB");

            if let Err(err) = conn.reducers.project_ship_locations(projection_timestamp()) {
                eprintln!("Failed to request ship projection: {err}");
            }
        })
        .on_connect_error(|_ctx, e| {
            eprintln!("Connection error: {:?}", e);
            std::process::exit(1);
        })
        .build()
        .expect("Failed to connect");

    conn.run_threaded();

    conn.subscription_builder()
        .on_applied(|ctx| {
            println!("Subscribed to ship and ship_projection tables");

            for ship in ctx.db().ship().iter() {
                println!("Ship: {} ({})", ship.name, ship.id);
            }

            for projection in ctx.db().ship_projection().iter() {
                let ship_name = ctx
                    .db()
                    .ship()
                    .id()
                    .find(&projection.ship_id)
                    .map(|ship| ship.name)
                    .unwrap_or_else(|| format!("ship-{}", projection.ship_id));

                println!(
                    "Existing projection for {}: lat={}, lon={} dead_reckoning={}",
                    ship_name, projection.lat, projection.lon, projection.used_dead_reckoning
                );
            }
        })
        .on_error(|_ctx, e| eprintln!("There was an error when subscribing: {e}"))
        .add_query(|q| q.from.ship())
        .add_query(|q| q.from.ship_projection())
        .subscribe();

    conn.db().ship().on_insert(|_ctx, ship| {
        println!("New ship: {} ({})", ship.name, ship.id);
    });

    conn.db().ship_projection().on_insert(|ctx, projection| {
        print_ship_projection(ctx, &projection);
    });

    conn.db().ship_projection().on_update(|ctx, _old, projection| {
        print_ship_projection(ctx, &projection);
    });
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
