use axum::{http::StatusCode, response::{IntoResponse, Json}};
use http::Request;
use log::info;
use pleco::Board;
use chrono::Utc;
use serde_json::json;
use crate::{authlayer, gameserver::Game, redislayer::RedisLayer};

pub async fn matchmaking_handler(req: Request<hyper::Body>) -> impl IntoResponse {
    
    let redislayer = RedisLayer::new().await;

    let user_id = match authlayer::get_jwt_sub(&req).await {
        Ok(id) => id,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message": format!("encountered error: {}", e.1)}))),
    };

    //logic to return early if user is already in matchmaking pool (otherwise they can spam the endpoint and skip the queue)
    let score: Option<f64> = match redislayer.zscore("matchmaking_pool", &user_id.to_string()).await {
        Ok(score) => score,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "message": format!("encountered error fetching matchmaking pool from redis: {}", e)}))),
    };

    if score.is_some() {
        return (StatusCode::BAD_REQUEST, Json(json!({"message": format!("User already in matchmaking pool")})));
    }

    //adding user to the pool
    let timestamp: i64 = Utc::now().timestamp();

    match redislayer.zadd("matchmaking_pool", &user_id.to_string(), timestamp as f64).await {
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
    let redislayer = RedisLayer::new().await;

    loop {
        let player_count: i32 = redislayer.zcard("matchmaking_pool").await.unwrap().try_into().unwrap();
        if player_count < 2 {
            continue;
        }
        let result: redis::RedisResult<Vec<(String, f64)>> = redislayer.zpopmin("matchmaking_pool", 2).await;

        match result {
            Ok(players) => {
                if !players.is_empty() {
                    let valid_players: Vec<(String, f64)> = players.into_iter()
                        .filter(|(user_id, _score)| !user_id.is_empty())  // Check if user_id is non-empty
                        .collect();
    
                    if !valid_players.is_empty() {
                        for (user_id, score) in &valid_players {
                            println!("Popped User: {}, Score (timestamp): {}", user_id, score);
                        }
                        create_game(valid_players[0].0.parse().unwrap(),
                        valid_players[1].0.parse().unwrap(), &redislayer).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error performing ZPOPMIN on matchmaking pool: {}", e);
            }
        }
    }
        

}

async fn create_game(player1: u32, player2: u32, redislayer: &RedisLayer) {
    let game_id: u32 = match redislayer.incr("game_id_counter").await {
        Ok(id) => id.try_into().unwrap(),
        Err(_) => return (), //handle this..
    };

    info!("game id counter: {}", game_id);

    let now = Utc::now().timestamp();

    let game = Game {
        game_id: game_id,
        player_white: player1,
        player_black: player2,
        game_created: now,
        game_initiated: 0,
        last_moved: (player2, now), //so white starts
        board_state: Board::start_pos().fen().to_string(),
        previous_move: None,
    };

    let _ = redislayer.hset_game(&game).await; //create game hashmap

    //these might need to be awaited so we dont make things in redis before others are available
    let _ = redislayer.zadd("active_games", &game_id.to_string(), now as f64).await; //active game pool

    //user->game mapping
    let res = redislayer.hset(&format!("user:{}",player1), "game_id", &game_id.to_string()).await;
    info!("result: {:?}", res);
    let _ = redislayer.hset(&format!("user:{}",player2), "game_id", &game_id.to_string()).await;

    info!("created game: {} for players: {}, {}", game_id, player1, player2);
}