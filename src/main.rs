use axum::{
    middleware, routing::{get, post}, Router
};
use tokio::task;
use tower_http::cors::{CorsLayer, Any};
use hyper::Method;
use http::HeaderName;

mod websocket;
mod matchmaking;
mod utils;
mod authlayer;
mod databaselayer;
use authlayer::validate_jwt_sub;
use websocket::websocket_handler;
use matchmaking::{matchmaking_handler, bot_handler, matchmaking_status, match_maker};

mod testing;
use testing::test_setup;

const HOST_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() {

    let cors = CorsLayer::new()
    .allow_origin(Any) 
    .allow_methods(vec![
        Method::GET,
        Method::POST,
        Method::OPTIONS,
    ])
    .allow_headers(vec![
        HeaderName::from_static("authorization"),
        HeaderName::from_static("content-type"),
    ]);

    // Spawning the concurrent thread to make matches
    task::spawn(async {
        match_maker().await;
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/matchmaking", post(matchmaking_handler))
        .route("/bot", post(bot_handler))
        .route("/matchmaking/:status_id", get(matchmaking_status))
        .route("/test", get(test_setup)).layer(middleware::from_fn(validate_jwt_sub)).layer(cors);


    // Run the Axum HTTP server concurrently
    axum::Server::bind(&HOST_ADDR.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}