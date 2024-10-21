use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};
use tokio::task;

use crate::{authlayer, utils::decode_user_id};
use crate::utils::user_id_to_game_id;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use log::info;

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
            let _ = stream
                .send(Message::Text(format!("Authentication failed")))
                .await;
            let _ = stream.close().await;
            return;
        }
    };

    //validated ok, carrying on:

    let (sender, receiver) = stream.split();

    let user_id = match authlayer::get_user_id_from_token(&token).await {
        Ok(id) => id,
        Err(e) => {
            info!("Failed to resolve userId from token: {}", e.1);
            return;
        }
    };

    info!("Authenticated user: {}", user_id);

    //check if there is a game in redis for that user id

    //game data:
    //game_id: u32
    //player_white: u32 (userId)
    //player_black: u32
    //game_created: timestamp
    //game_initiated: bool
    //last_moved: (id, timestamp)
    //TODO: time_remaining_white: 5mins...


    task::spawn(async move {
        message_receiver(receiver, user_id).await;
    });

    task::spawn(async move {
        message_sender(sender, user_id).await;
    });
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

async fn message_receiver(mut receiver: SplitStream<WebSocket>, user_id: u32) {
    while let Some(Ok(message)) = receiver.next().await {
        if let Message::Text(text) = message {
            // do something
            info!("message from client: {}", text);
        }
    }    
}

async fn message_sender(mut sender: SplitSink<WebSocket, Message>, user_id: u32) {
    let _ = sender.send(Message::Text(format!("Successfully authenticated user: {}", user_id))).await;
}

async fn handle_message(message: String, user_id: usize) {
    //let game_id = user_id_to_game_id(user_id);

    //parse message as EventMessage struct

    // match message.event {
    //     "game_move" => handle_move(message.data, game_id, user_id),
    //     "game_surrender" => handle_surrender(message.data, game_id, user_id),
    //     "game_reminder" => handle_reminder(message.data, game_id), //re-send the game state if client needs it
    //     Err(e) => log!("Error when parsing message: {}", e),
    // }
}

// async fn handle_move(data: EventData, game_id: usize, user_id: usize) {
//     //check the Active Games collection in Redis
//     // -> ensure that 'last_moved' != user_id (otherwise its not our turn)

//     //recreate game state in Pleco using fen from Active Games
//     //validate move (going to need a whole chess layer to do this but ive already written most of it)
//     //make the move on the board object
//     //publish new game state to the 'game_updates: {game_id}' channel in redis
//     //update 'last_moved' to user_id for the game

//     //send the new board state with some sort of success message over ws (maybe pass this back up the chain?)

// }

// async fn handle_surrender(data: EventData, game_id: usize, user_id: usize) {
//     //TODO
//     //...
// }

// async fn game_reminder(data: EventData, game_id: usize, user_id: usize) {
//     //TODO
//     //...
// }


//TODO: need a struct for sending data to client

//TODO: need a function for parsing the data from redis into the json to send to the client

//JSON Format
// Note: when we use serde to parse the message, we parse it into the EventMessage struct, so if a message doesn't have this format, it will fail

// #[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
// struct EventMessage {
//     event: String, //eg: game_start, game_move, game_surrender
//     data: EventData,
// }

// #[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
// struct EventData {
//     player: PlayerColour,
//     from: Option<String>,
//     to: Option<String>,
//     flags: Option<String>,
//     status: Option<String>,
// }

// #[derive(Serialize, Deserialize, Debug)]
// enum PlayerColour {
//     White,
//     Black,
// }

// impl Not for PlayerColour {
//     type Output = PlayerColour;

//     fn not(self) -> PlayerColour {
//         match self {
//             PlayerColour::White => PlayerColour::Black,
//             PlayerColour::Black => PlayerColour::White,
//         }
//     }
// }

