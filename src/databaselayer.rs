use mysql::*;
use mysql::prelude::*;
use uuid::Uuid;
use std::env;
use axum::response::{IntoResponse};
use axum::http::StatusCode;

pub fn connect_to_db() -> Result<PooledConn, Box<dyn std::error::Error>> {
    let url = env::var("DATABASE_URL")?;
    let opts = Opts::from_url(&url)?;
    let pool = Pool::new(opts)?;
    let conn = pool.get_conn()?;  
    Ok(conn)
}

pub fn get_user_id_by_external_user_id(conn: &mut PooledConn, external_user_id: &str) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    let query = "SELECT id FROM users WHERE external_user_id = ?";
    let result: Option<(u32,)> = conn.exec_first(query, (external_user_id,))?;

    match result {
        Some((id,)) => Ok(Some(id)),
        None => Ok(None),
    }
}

pub fn create_user(conn: &mut PooledConn, external_user_id: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let uuid = Uuid::new_v4().to_string();

    let query = r"INSERT INTO users (external_user_id, uuid) VALUES (?, ?)";
    conn.exec_drop(query, (external_user_id, &uuid))?;

    match get_user_id_by_external_user_id(conn, external_user_id)? {
        Some(user_id) => Ok(user_id),
        None => Err("Failed to retrieve newly created user".into()),
    }
}
