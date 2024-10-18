use axum::{http::StatusCode, response::{IntoResponse, Json}};
use http::Request;
use redis::AsyncCommands;
use uuid::timestamp;
use std::env;
use chrono::{Utc};
use serde_json::json;
use dotenv::dotenv;
use crate::authlayer;
use crate::databaselayer::redis_connection;

pub async fn matchmaking_handler(req: Request<hyper::Body>) -> impl IntoResponse {
    
    let mut con = match redis_connection().await {
        Ok(c) => c,
        Err((status, json)) => return (status, json),
    };

    let user_id = match authlayer::get_jwt_sub(&req).await {
        Ok(id) => id,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message": format!("encountered error: {}", e.1)}))),
    };

    //logic to return early if user is already in matchmaking pool (otherwise they can spam the endpoint and skip the queue)
    let score: Option<f64> = con.zscore("matchmaking_pool", &user_id).await.unwrap();

    if score.is_some() {
        return (StatusCode::BAD_REQUEST, Json(json!({"message": format!("User already in matchmaking pool")})));
    }

    //adding user to the pool
    let timestamp: i64 = Utc::now().timestamp();

    match con.zadd("matchmaking_pool", user_id, timestamp as f64).await {
        Ok(()) => {
            return (StatusCode::OK, Json(json!({
                "message": "User has been added to matchmaking pool successfully",
                "instructions": "Query GET /matchmaking for an update on matchmaking status",
            })));
        }
        Err(e) => {
            eprintln!("Error adding to Redis ZSET: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message": "Failed to add to Redis matchmaking pool"})))
        }
    }

    //temp code to check the pool
    // let members: Vec<(String, i64)> = con.zrange_withscores("matchmaking_pool", 0, -1).await.expect("failed to get members in matchmaking pool");
    // for (member, score) in members {
    //     println!("User: {}, Timestamp: {}", member, score);
    // }
}

pub async fn bot_handler() {
 // TODO
}

pub async fn matchmaking_status() {
    //checks active game pool for user_id (maybe also task id, might not even need this)

    //if there is an active game, return 200 OK, user should switch to websocket

    //if no game, return HTTP 102
}

pub async fn match_maker() {
    //on loop:
        //BRPOP the matchmaking pool in redis, twice, getting a player each time

        //if both players are Some(), then remvoe from the pool (if BRPOP doesnt already do that?) and make a new game
        //  - Add the game to the Active Games collection in redis
}