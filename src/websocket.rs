use std::{env, sync::Arc};
use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task};
use crate::{authlayer, gameserver::{self, GameServer, Game}, redislayer::{self, RedisLayer}, utils::decode_user_id};
use crate::utils::user_id_to_game_id;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use log::info;
use redis::PubSub;
// extern crate pleco;
use pleco;
use pleco::{core::piece_move::{MoveFlag, PreMoveInfo}, BitMove, Board, PieceType, SQ};
use dotenv::dotenv;
use std::time::Duration;
use std::thread::sleep;

pub async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut stream: WebSocket) { //also takes the token here

    let token: String = match listen_for_token(&mut stream).await {
        Some(token) => token,
        None => {
            //TODO: need to properly handle closing the stream, if a game is open, the other player should be informed and the game closed.
            // or we have some sort of re-connection within a time window
            //note to self: when a game is created, players have N mins to join before the game expires
            let _ = stream.send(Message::Text(format!("Authentication failed"))).await;
            let _ = stream.close().await;
            return;
        }
    };

    let user_id = match authlayer::get_user_id_from_token(&token).await {
        Ok(id) => id,
        Err(e) => {
            info!("Failed to resolve userId from token: {}", e.1);
            return;
        }
    };
    info!("Authenticated user: {}", user_id);

    //establish redis con, which is shared between threads later on
    let redis_layer = redislayer::RedisLayer::new().await;

    let game_id = match redis_layer.hget(&format!("user:{}", user_id), "game_id").await {  //check if there is a game in redis for that user id
        Some(game_id) => game_id,
        None => {
            let _ = stream.send(Message::Text(format!("User has no associated game"))).await;
            let _ = stream.close().await;
            return;
        }
    }; // TODO: clean up old user id -> game id mappings on close
    let game_id = match game_id.parse::<u32>() {
        Ok(game_id) => game_id,
        Err(e) => {
            info!("Error parsing game_id: {}", e);
            return;
        }
    };
    info!("game id: {}", game_id);

    let game: Game = match redis_layer.get_game(game_id).await {
        Some(game) => game,
        None => {
            info!("Failed to get game");
            return;
        }
    };

    info!("Found game id: {} for user: {}", game_id, user_id);

    if let Err(e) = ready_up(game, user_id, &redis_layer).await {
        info!("Encountered Error waiting for game {} to start for user: {}: {}", game_id, user_id, e);
        return;
    }

    //Spin up send / receive threads
    let (sender, receiver) = stream.split();
    let sender: Arc<Mutex<SplitSink<WebSocket, Message>>> = Arc::new(Mutex::new(sender));

    info!("Spinning up send/receive for user: {}", user_id);

    task::spawn({
        let sender = sender.clone();
        async move {
            message_receiver(receiver, sender, user_id, game_id).await;
        }
    });

    task::spawn({
        let sender = sender.clone();
        async move {
            gameserver::message_sender(sender, user_id, game_id).await;
        }
    });

    // task::spawn({
    //     let sender = sender.clone();
    //     async move {
    //         loop {
    //             // info!("pinger waiting for sender lock..");
    //             let mut sender = sender.lock().await;
    //             // info!("pinger got sender lock");
    //             let sender_status = sender.send(Message::Text("PING".to_string())).await;
    //             match sender_status {
    //                 Ok(o) => //info!("sent ping ok"),
    //                 Err(e) => //info!("error sending ping"),
    //             }
    //             sleep(Duration::from_millis(1000));
    //         }
    //     }
    // });
}

async fn ready_up(game: Game, user_id: u32, redislayer: &redislayer::RedisLayer) -> Result<(), String> {
    let opponent_id = if game.player_white == user_id {game.player_black} else {game.player_white};

    let hset_result = redislayer.hset(&format!("game_readiness:{}", game.game_id), &user_id.to_string(), "ready").await;
    info!("user {} is ready... {:?}", user_id, hset_result);

    loop {
        let result = redislayer.hget(&format!("game_readiness:{}", game.game_id), &opponent_id.to_string()).await;
        if result.is_some() {
            info!("opponent ready!");
            //initiate game
            let timestamp = Utc::now().timestamp().to_string();

            redislayer.hset(
                &format!("game:{}", game.game_id),
                "game_initiated",
                &timestamp
            ).await.expect("Failed to set game as initiated");
            break;
        }
    }
    Ok(())
}

async fn listen_for_token(stream: &mut WebSocket) -> Option<String> {
    if let Some(Ok(Message::Text(text))) = stream.next().await {
        let data: serde_json::Value = serde_json::from_str(&text).ok()?;
        let token_value = data.get("token")?.to_string();
        let token_value = token_value.trim_matches('"').to_string();

        if authlayer::validate_token(&token_value.as_str()).await.is_ok() {
            info!("Authenticated via WebSocket");
            return Some(token_value);
        }
    }

    None
}

async fn message_receiver(mut receiver: SplitStream<WebSocket>, sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, user_id: u32, game_id: u32) {
    let gameserver = GameServer::new(game_id, user_id).await;

    while let Some(message_result) = receiver.next().await {
        match message_result {
            Ok(message) => {
                // info!("Message received: {:?}", message);
                match message {
                    Message::Text(text) => {
                        if text == "0" { //handle PING, (sent as a "0")
                            // info!("waiting for sender lock (message receiver)");
                            let mut sender = sender.lock().await;
                            // info!("(message rec) got lock");
                            let _send_result = sender.send(Message::Text("PONG".to_string())).await; //TODO: fail if this fails
                            // match send_result {
                            //     Ok(o) => info!("sent PONG"),
                            //     Err(e) => info!("error sending PONG"),
                            // }
                        continue;
                        }
                        gameserver.handle_received_message(text).await;
                    },
                    Message::Close(reason) => {
                        info!("Close message received: {:?}", reason);
                        let mut sender = sender.lock().await;
                        let _ = sender.send(Message::Close(reason)).await;
                        info!("Connection closed by client");
                        break;
                    },
                    _ => {
                        info!("Unknown message type received");
                    },
                }
            },
            Err(e) => {
                info!("Error receiving message: {}", e);
                // Check if the connection was closed or another error occurred
                break; // Exit the loop on error
            }
        }
    }
    info!("Exited message receiving loop for user: {}", user_id);
}



