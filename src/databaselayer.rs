use mysql::*;
use mysql::prelude::*;
use redis::aio::MultiplexedConnection;
use serde_json::json;
use uuid::Uuid;
use core::time;
use std::{collections::HashMap, env};
use axum::{http::StatusCode, response::{IntoResponse, Json}};
use redis::AsyncCommands;
use chrono::{Utc};
use dotenv::dotenv;


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

pub async fn redis_connection() -> Result<MultiplexedConnection, (StatusCode, Json<serde_json::Value>)> {
    dotenv().ok();

    let redis_url = env::var("REDIS_URL").unwrap();
    
    let client = match redis::Client::open(redis_url) {
        Ok(c) => c,
        Err(_) => return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message": "Failed to connect to Redis"})))),
    };
    
    let mut con = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(_) => return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message": "Failed to get Redis connection"})))),
    };

    Ok(con)
}

pub async fn create_game(player_1: u32, player_2: u32) {
    let mut con = match redis_connection().await {
        Ok(c) => c,
        Err((_status, _json)) => {
            println!("Failed connecting to redis! (create_game)");
            return ();
        },
    };

    let game_id: u32 = match con.incr("game_id_counter",1).await {
        Ok(id) => id,
        Err(_) => return (), //handle this..
    };

    let timestamp: i64 = Utc::now().timestamp();

    let result: () = con.hset_multiple(
        format!("game:{}", game_id),
        &[
            ("player_white", player_1.to_string()), // player IDs are integers, store them as strings
            ("player_black", player_2.to_string()),
            ("game_created", timestamp.to_string()), // Timestamp stored as string
            ("game_initiated", 0.to_string()), // Game initiated state stored as a string
            ("last_moved", format!("{} {}", player_2, timestamp)), // last_moved stored as a string
        ]
    ).await.unwrap();

    let _: () = con.zadd(
        "active_games",
        game_id,
        timestamp
    ).await.unwrap();

    //mapping for user_id -> game:
    //TODO- need to prune when a game is closed
    let _: () = con.hset(
        format!("user:{}", player_1),
        "game_id",
        game_id.to_string()
    ).await.unwrap();

    let _: () = con.hset(
        format!("user:{}", player_2),
        "game_id",
        game_id.to_string()
    ).await.unwrap();

    println!("created game: {} for players: {}, {}", game_id, player_1, player_2);

    ()
}

pub async fn user_id_to_game_id(user_id: u32) -> Option<u32> {
    let mut con = match redis_connection().await {
        Ok(c) => c,
        Err((_status, _json)) => {
            println!("Failed connecting to redis! (user id to game id)");
            return None; //handle this properly
        },
    };

    let user_game_id: Option<String> = con.hget(format!("user:{}", user_id), "game_id").await.unwrap();

    match user_game_id {
        Some(game_id) => game_id.parse::<u32>().ok(),
        None => None,
    }
}

pub async fn get_game(game_id: u32) -> Option<HashMap<String, String>> {
    let mut con = match redis_connection().await {
        Ok(c) => c,
        Err((_status, _json)) => {
            println!("Failed connecting to redis! (get game)");
            return None; //handle this properly
        },
    };

    let game_key = format!("game:{}", game_id);
    //handle err better
    let game_data: HashMap<String, String> = con.hgetall(&game_key).await.expect("Failed getting game");
    
    Some(game_data)
}

struct Game {
    game_id: u32,
    player_white: u32,
    player_black: u32,
    game_created: i64,
    game_initiated: i64,
    last_moved: (u32, i64), // (user_id, timestamp)
}
