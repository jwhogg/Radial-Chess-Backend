use axum::{http::StatusCode, response::{IntoResponse, Json, Response}};
use http::{header, Method, Request};
use hyper::Body;
use log::info;
use pleco::Board;
use chrono::Utc;
use serde_json::json;
use crate::{authlayer, gameserver::Game, redislayer::{self, RedisLayer}};

pub async fn matchmaking_options(req: Request<hyper::Body>) -> impl IntoResponse {
    info!("hit matchmaking options");
    if req.method() == Method::OPTIONS { //respond to preflight request
        info!("OPTIONS responding to preflight check!");
        return cors_response(StatusCode::OK, json!({"message": "Preflight request OK"}));
    } else {
        return cors_response(StatusCode::INTERNAL_SERVER_ERROR, json!({"message": "Request method is not OPTIONS!"}))
    }
}

pub async fn matchmaking_handler(req: Request<hyper::Body>) -> impl IntoResponse {
    info!("post /matchmaking hit!");

    let redislayer = RedisLayer::new().await;

    let user_id = match authlayer::get_jwt_sub(&req).await {
        Ok(id) => id,
        Err(e) => {
            return cors_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": format!("encountered error: {}", e.1)}),
            );
        }
    };

    // Logic to return early if user is already in matchmaking pool
    let score: Option<f64> = match redislayer.zscore("matchmaking_pool", &user_id.to_string()).await {
        Ok(score) => score,
        Err(e) => {
            return cors_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": format!("encountered error fetching matchmaking pool from redis: {}", e)}),
            );
        }
    };

    if score.is_some() {
        return cors_response(
            StatusCode::BAD_REQUEST,
            json!({"message": "User already in matchmaking pool"}),
        );
    }

    // Adding user to the pool
    let timestamp: i64 = Utc::now().timestamp();
    match redislayer.zadd("matchmaking_pool", &user_id.to_string(), timestamp as f64).await {
        Ok(()) => {
            return cors_response(
                StatusCode::OK,
                json!({
                    "message": "User has been added to matchmaking pool successfully",
                    "instructions": "Query GET /matchmaking for an update on matchmaking status",
                }),
            );
        }
        Err(e) => {
            eprintln!("Error adding to Redis ZSET: {:?}", e);
            return cors_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": "Failed to add to Redis matchmaking pool"}),
            );
        }
    }
}

pub async fn bot_handler() {
 // TODO
}

pub async fn player_stats(req: Request<hyper::Body>) -> impl IntoResponse {

    if req.method() == Method::OPTIONS { //respond to preflight request
        info!("OPTIONS responding to preflight check!");
        return cors_response(StatusCode::OK, json!({"message": "Preflight request OK"}));
    }

    info!("hit player stats");
    let redislayer = RedisLayer::new().await;

    let user_id = match authlayer::get_jwt_sub(&req).await {
        Ok(id) => id,
        Err(e) => {
            return cors_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": format!("encountered error: {}", e.1)}),
            );
        }
    };

    info!("user id: {}", user_id);

    let wins = redislayer.hget(&format!("player_stats:{}",user_id), "wins").await.unwrap_or("0".to_string());
    let draws = redislayer.hget(&format!("player_stats:{}",user_id), "draws").await.unwrap_or("0".to_string());
    let losses = redislayer.hget(&format!("player_stats:{}",user_id), "losses").await.unwrap_or("0".to_string());
    info!("wins: {}", wins);
    return cors_response(StatusCode::OK, json!({"wins": wins, "draws": draws, "losses": losses}));

}

pub async fn matchmaking_status(req: Request<hyper::Body>) -> impl IntoResponse {
    info!("GET matchmaking status hit!");

    let user_id = match authlayer::user_id_from_request(&req).await {
        Some(id) => id,
        None => return cors_response(StatusCode::BAD_REQUEST, json!({"message": "Failed to find user from provided bearer token"})),
    };

    let redislayer = RedisLayer::new().await;

    let game_id = match redislayer.hget(&format!("user:{}", user_id), "game_id").await {  //check if there is a game in redis for that user id
        Some(game_id) => game_id,
        None => {
            // accepted 202 means response is still processing
            return cors_response(StatusCode::ACCEPTED, json!({"message": "User is waiting in the matchmaking pool..."}));
        }
    };

    match game_id.parse::<u32>() { //see if game id can be succesfully parsed (to handle redis returning something like "[]")
        Ok(game_id) => return cors_response(StatusCode::OK, json!({
            "message": format!("Found game: {} for user", game_id),
            "instructions": "Open a websocket request to the server at /ws"
        })),
        Err(e) => { //use the e here
            return cors_response(StatusCode::ACCEPTED, json!({"message": "User is waiting in the matchmaking pool..."}));
        }
    };
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

fn cors_response(status: StatusCode, body: serde_json::Value) -> Response<Body> {
    let json_body = body.to_string();
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*") // Allow requests from any origin
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS")
        .header(header::CONTENT_TYPE, "application/json") // Set Content-Type to JSON
        .body(Body::from(json_body))
        .unwrap() // Or handle error more gracefully
}