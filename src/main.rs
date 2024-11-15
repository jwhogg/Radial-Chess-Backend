use axum::{
    middleware, routing::{get, post, options}, Router
};
use tokio::task;
use tower_http::cors::{Any, CorsLayer};
use hyper::Method;
use http::HeaderName;
use std::env;

mod websocket;
mod matchmaking;
mod utils;
mod authlayer;
mod databaselayer;
mod redislayer;
mod gameserver;
use authlayer::validate_jwt_sub;
use websocket::websocket_handler;
use matchmaking::{bot_handler, match_maker, matchmaking_handler, matchmaking_options, matchmaking_status, player_stats};

mod testing;
use testing::test_setup;

const DEFAULT_HOST_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() {

    env_logger::init();

    let host_addr = env::var("HOST_ADDR").unwrap_or_else(|_| DEFAULT_HOST_ADDR.to_string());

    // Spawning the concurrent thread to make matches
    task::spawn(async {
        match_maker().await;
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/matchmaking", post(matchmaking_handler))
        .route("/matchmaking", options(matchmaking_options))
        .route("/playerstats", options(player_stats))
        .route("/playerstats", get(player_stats))
        // .route("/bot", post(bot_handler))
        .route("/test", get(test_setup))
        .route("/matchmaking", get(matchmaking_status).layer(middleware::from_fn(validate_jwt_sub)));
        // .layer(cors);
        // .route("/test", get(test_setup)).layer(CorsLayer::very_permissive()).layer(middleware::from_fn(validate_jwt_sub));


    // Run the Axum HTTP server concurrently
    axum::Server::bind(&host_addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}