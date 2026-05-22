use spacetimedb::Timestamp;

#[spacetimedb::table(accessor = global_state, public)]
pub struct GlobalState {
    // just one row
    #[primary_key]
    pub id: u8,

    pub oldest: Option<Timestamp>,
    pub newest: Option<Timestamp>,
}
