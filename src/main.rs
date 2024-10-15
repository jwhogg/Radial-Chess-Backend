use axum::{
    routing::{get, post},
    Router,
};
use tokio::task;

mod websocket;
mod matchmaking;
mod utils;
use websocket::websocket_handler;
use matchmaking::{matchmaking_handler, bot_handler, matchmaking_status, match_maker};

mod testing;
use testing::testSetup;

const HOST_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() {

    task::spawn(async {
        match_maker().await;
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler)) // Responds to GET requests at "/"
        .route("/matchmaking", post(matchmaking_handler)) // Responds to POST requests at "/post"
        .route("/bot", post(bot_handler)) // Responds to POST requests at "/post"
        .route("/matchmaking/:status_id", get(matchmaking_status))
        .route("/test", get(testSetup));


    // Run the Axum HTTP server concurrently
    axum::Server::bind(&HOST_ADDR.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}