#[derive(Clone)]
pub struct AppState {
    pub db: paybank_db::Db,
}

impl AppState {
    pub fn new(db: paybank_db::Db) -> Self {
        Self { db }
    }
}