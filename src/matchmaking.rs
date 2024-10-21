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
    let score: Option<f64> = match con.zscore("matchmaking_pool", &user_id).await {
        Ok(score) => score,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "message": format!("encountered error fetching matchmaking pool from redis: {}", e)}))),
    };

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
        //ZPOPMIN "matchmaking_pool" 2

        //if both players are Some(), then remvoe from the pool (if BRPOP doesnt already do that?) and make a new game
        //  - Add the game to the Active Games collection in redis
    
    let mut con = match redis_connection().await {
        Ok(c) => c,
        Err((status, json)) => return (),
    };

    loop {
        let player_count: i32 = con.zcard("matchmaking_pool").await.unwrap();
        if player_count < 2 {
            continue;
        }
        let result: redis::RedisResult<Vec<(String, f64)>> = con.zpopmin("matchmaking_pool", 2).await;

        match result {
            Ok(players) => {
                if !players.is_empty() {
                    let valid_players: Vec<(String, f64)> = players.into_iter()
                        .filter(|(user_id, _score)| !user_id.is_empty())  // Check if user_id is non-empty
                        .collect();
    
                    if !valid_players.is_empty() {
                        for (user_id, score) in valid_players {
                            println!("Popped User: {}, Score (timestamp): {}", user_id, score);
                        }
                        //create_game(valid_players.0, valid_players.1);
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error performing ZPOPMIN on matchmaking pool: {}", e);
            }
        }
    }
        

}