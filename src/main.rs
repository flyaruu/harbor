mod module_bindings;
use module_bindings::*;
use std::env;
use std::time::Duration;

use spacetimedb_sdk::{DbContext, Table};

fn add_named_ships_if_missing(ctx: &SubscriptionEventContext) {
    for ship_name in ["Wayfarer", "Northwind", "Evening Tide"] {
        let ship_exists = ctx.db().ship().iter().any(|ship| ship.name == ship_name);

        if ship_exists {
            continue;
        }

        if let Err(err) = ctx.reducers().add_ship(ship_name.to_string()) {
            eprintln!("Failed to add ship '{ship_name}': {err}");
        }
    }
}

fn report_ship_locations(conn: &DbConnection) {
    for ship in conn.db().ship().iter() {
        let lat = ship.id as f64;
        let lon = -(ship.id as f64);

        if let Err(err) = conn.reducers().add_location_report(ship.id, lat, lon) {
            eprintln!(
                "Failed to add location report for ship '{}': {err}",
                ship.name
            );
        }
    }
}

fn main() {
    // The URI of the SpacetimeDB instance hosting our chat module.
    let host: String = env::var("SPACETIMEDB_HOST").unwrap_or("http://localhost:3000".to_string());

    // The module name we chose when we published our module.
    let db_name: String = env::var("SPACETIMEDB_DB_NAME").unwrap_or("my-db".to_string());
    eprintln!(
        "Connecting to SpacetimeDB at: {} with database: {}",
        host, db_name
    );

    // Connect to the database
    let conn = DbConnection::builder()
        .with_database_name(db_name)
        .with_uri(host)
        .on_connect(|_, _, _| {
            println!("Connected to SpacetimeDB");
        })
        .on_connect_error(|_ctx, e| {
            eprintln!("Connection error: {:?}", e);
            std::process::exit(1);
        })
        .build()
        .expect("Failed to connect");

    conn.run_threaded();

    // Subscribe to the tables used by the client.
    conn.subscription_builder()
        .on_applied(|ctx| {
            println!("Subscribed to ship and location_reporter tables");
            add_named_ships_if_missing(ctx);
        })
        .on_error(|_ctx, e| eprintln!("There was an error when subscribing: {e}"))
        .add_query(|q| q.from.ship())
        .add_query(|q| q.from.location_reporter())
        .subscribe();

    // Register a callback for when rows are inserted into the ship table
    conn.db().ship().on_insert(|_ctx, ship| {
        println!("New ship: {}", ship.name);
    });

    conn.db().location_reporter().on_insert(|_ctx, report| {
        println!(
            "Ship {} location report: lat={}, lon={}",
            report.ship_id, report.lat, report.lon
        );
    });

    // Keep the main thread alive so the connection stays open
    loop {
        report_ship_locations(&conn);
        std::thread::sleep(Duration::from_secs(1));
    }
}
