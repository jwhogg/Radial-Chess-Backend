use std::{env, sync::Arc};
use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task};
use crate::{authlayer, databaselayer::{self, Game}, gameserver::{self, GameServer}, redislayer::{self, RedisLayer}, utils::decode_user_id};
use crate::utils::user_id_to_game_id;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use log::info;
use redis::PubSub;
// extern crate pleco;
use pleco;
use pleco::{core::piece_move::{MoveFlag, PreMoveInfo}, BitMove, Board, PieceType, SQ};
use dotenv::dotenv;

pub async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut stream: WebSocket) -> Result<(), Box<dyn std::error::Error>> { //also takes the token here

    let token: String = match listen_for_token(&mut stream).await {
        Some(token) => token,
        None => {
            //TODO: need to properly handle closing the stream, if a game is open, the other player should be informed and the game closed.
            // or we have some sort of re-connection within a time window
            //note to self: when a game is created, players have N mins to join before the game expires
            let _ = stream.send(Message::Text(format!("Authentication failed"))).await;
            let _ = stream.close().await;
            return Err("Auth failed".into());
        }
    };

    let user_id = match authlayer::get_user_id_from_token(&token).await {
        Ok(id) => id,
        Err(e) => {
            info!("Failed to resolve userId from token: {}", e.1);
            return Err("Failed to resolve userId".into());
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
            return Err("User has no associated game".into());
        }
    };
    let game_id = match game_id.parse::<u32>() {
        Ok(game_id) => game_id,
        Err(e) => {
            info!("Error parsing game_id: {}", e);
            return Err("Error parsing game_id".into());
        }
    };

    let game: Game = match redis_layer.get_game(game_id).await {
        Some(game) => game,
        None => {
            info!("Failed to get game");
            return Err("failed to get game".into());
        }
    };

    info!("Found game id: {} for user: {}", game_id, user_id);

    if let Err(e) = ready_up(game, user_id, &redis_layer).await {
        info!("Encountered Error waiting for game {} to start for user: {}: {}", game_id, user_id, e);
        return Err("Encountered Error waiting for game to start".into());
    }

    //Spin 
    let (sender, receiver) = stream.split();
    let sender: Arc<Mutex<SplitSink<WebSocket, Message>>> = Arc::new(Mutex::new(sender));
    let receiver = Arc::new(Mutex::new(receiver));
    // let (tx, mut rx) = mpsc::channel(32);

    // //task to relay any sent messages using the actual WebSocket sender to avoid problems with owning between threads
    // //this is kind of hacky, but sender doesnt implement sync, so we cant use between threads even with Arc(Mutex())
    // task::spawn(async move {
    //     let sender = Arc::new(Mutex::new(sender));
    //     while let Some(message) = rx.recv().await {
    //         let mut sender_lock = sender.lock().await;
    //         let _ = sender_lock.send(message).await;
    //     }
    // });

    task::spawn({
        let sender = sender.clone();
        let receiver = receiver.clone();
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
    Ok(())
}

async fn ready_up(game: Game, user_id: u32, redislayer: &redislayer::RedisLayer) -> Result<(), String> {
    let channel = &format!("game_updates:{}",game.game_id);
    let ready_message = format!("ready:{}:{}", user_id, true); //ready=true
    redislayer.publish(channel, &ready_message).await;

    let opponent_id = if game.player_white == user_id {game.player_black} else {game.player_white};

    let mut c = redislayer.con_for_subscribe();
    let mut sub = c.as_pubsub();
    sub.subscribe(channel).expect("failed subscribing to channel");

    loop {
        //expect messages to be "in:this:form", we want something like "ready:8:true"
        let msg = sub.get_message().expect("Failed to receive message"); //get message is a blocking action
        let payload: String = msg.get_payload().expect("Failed to get payload");
        let parts: Vec<&str> = payload.split(':').collect();
        if parts.len() == 3
            && parts[0] == "ready"
            && parts[1].parse::<u32>().unwrap_or(0) == opponent_id 
            && parts[2].parse().unwrap_or(false)
        {
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

async fn message_receiver(receiver: Arc<tokio::sync::Mutex<SplitStream<WebSocket>>>, sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, user_id: u32, game_id: u32) {
    let gameserver = GameServer::new(game_id, user_id).await;

    let mut receiver = receiver.lock().await;

    while let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(text) => {
                gameserver.handle_received_message(text).await;
            },
            Message::Ping(ping) => {
                let mut sender = sender.lock().await;
                let _ = sender.send(Message::Pong(ping)).await;
            },
            Message::Pong(pong) => {
                let mut sender = sender.lock().await;
                let _ = sender.send(Message::Ping(pong)).await;
            },
            Message::Close(reason) => {
                // Handle closure by responding with a close frame
                //TODO: logic for closing the game- inform opponent via redis, inform the counterpart message_sender function
                let mut sender = sender.lock().await;
                let _ = sender.send(Message::Close(reason)).await;
                //gameserver.close();
                info!("Connection closed");
                break;
            },
            _ => {},
        }
    }    
}



