mod module_bindings;
use module_bindings::*;
use std::env;
mod ais;

use spacetimedb_sdk::{DbContext, Table};

use crate::ais::run_ais;

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

    conn.subscription_builder()
        .on_applied(|_ctx| {
            println!("Subscribed to ship and location_reporter tables");
            // add_named_ships_if_missing(ctx);
        })
        .on_error(|_ctx, e| eprintln!("There was an error when subscribing: {e}"))
        .add_query(|q| q.from.ship())
        .add_query(|q| q.from.location_report())
        .subscribe();

    conn.db().ship().on_insert(|_ctx, ship| {
        println!("New ship: {}", ship.name);
    });

    conn.db().location_report().on_insert(|_ctx, report| {
        println!(
            "Ship {} location report: lat={}, lon={}",
            report.ship_id, report.lat, report.lon
        );
    });

    run_ais(conn).unwrap_or_else(|e| eprintln!("Error running AIS stream: {e}"));

    // loop {
    //     report_ship_locations(&conn);
    //     std::thread::sleep(Duration::from_secs(1));
    // }
}
