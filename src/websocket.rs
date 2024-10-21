use std::sync::Arc;

use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task};

use crate::{authlayer, databaselayer, utils::decode_user_id};
use crate::utils::user_id_to_game_id;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use log::info;
use redis::PubSub;
extern crate pleco;
use pleco::{core::piece_move::{MoveFlag, PreMoveInfo}, BitMove, Board, PieceType, SQ};

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

    let user_id = match authlayer::get_user_id_from_token(&token).await {
        Ok(id) => id,
        Err(e) => {
            info!("Failed to resolve userId from token: {}", e.1);
            return;
        }
    };

    info!("Authenticated user: {}", user_id);

    //check if there is a game in redis for that user id

    let game_id = match databaselayer::user_id_to_game_id(user_id).await {
        Some(game_id) => game_id,
        None => {
            println!("user has no associated game"); //handle properly,
            return;
        }
    };

    let (sender, receiver) = stream.split();

    let sender = Arc::new(Mutex::new(sender));
    
    loop {
        //need to add a clause for game expiry
        if let Some(game) = databaselayer::get_game(game_id).await {
            let mut player_colour = "";
            let mut opponent_ready = false;
            if game.player_white == user_id {
                player_colour = "white";
                opponent_ready = game.player_black_ready;
            } else if game.player_black == user_id {
                player_colour = "black";
                opponent_ready = game.player_white_ready;
            }
            databaselayer::set_player_colour_ready(player_colour, game_id, true).await;

            if opponent_ready {
                databaselayer::initiate_game(game_id).await;
                break;
            }
        }
    }

    task::spawn({
        let sender_clone = Arc::clone(&sender);
        async move {
            message_receiver(receiver, sender_clone, user_id, game_id).await;
        }
    });

    task::spawn({
        let sender_clone = Arc::clone(&sender);
        async move {
            message_sender(sender_clone, user_id, game_id).await;
        }
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

async fn message_receiver(mut receiver: SplitStream<WebSocket>, mut sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, user_id: u32, game_id: u32) {
    while let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(text) => {
                handle_received_message(text, user_id, game_id).await;
            },
            Message::Ping(ping) => {
                let mut sender = sender.lock().await;
                sender.send(Message::Pong(ping)).await;
            },
            Message::Pong(pong) => {
                let mut sender = sender.lock().await;
                sender.send(Message::Ping(pong)).await;
            },
            Message::Close(reason) => {
                // Handle closure by responding with a close frame
                let mut sender = sender.lock().await;
                sender.send(Message::Close(reason)).await;
                info!("Connection closed");
            },
            _ => {},
        }
    }    
}

async fn handle_received_message(msg: String, user_id: u32, game_id: u32) {
    //get current board fen from redis x
    //check last_played != user_id x
    //in the future: check the timer
    //load the fen into pleco x
    //validate the move x
    //update the Game hashmap (fen, last_moved) x
    //publish on the channel

    //parse msg (move, surreder, offer draw, reminder)
    let parsed_message: EventMessage = match serde_json::from_str(&msg) {
        Ok(m) => m,
        Err(e) => {
            info!("Failed to parse JSON");
            return;
        },
    };

    match parsed_message.event.as_str() {
        "game_move" => handle_move(parsed_message.data, user_id, game_id).await,
        "game_surrender" => handle_surrender(user_id, game_id),
        "game_offer_draw" => handle_offer_draw(user_id, game_id),
        "game_accept_draw" => handle_accept_draw(user_id, game_id),
        "game_decline_draw" => handle_decline_draw(user_id, game_id),
        "game_reminder" => handle_send_reminder(user_id, game_id),
        _ => (),
    }
}

async fn handle_move(data: EventData, user_id: u32, game_id: u32) {
    let game = match databaselayer::get_game(game_id).await {
        Some(game) => game,
        None => {
            info!("Failed to retreive game (handle_received_message)");
            return;
        },
    };

    if game.last_moved.0 == user_id {
        info!("Invalid! Player has already taken their turn");
        return;
    }

    let mut board: Board = Board::from_fen(&game.board_state).expect("Failed to load board state");

    let bit_move = match construct_bit_move(data, &board) {
        Ok(bmove) => bmove,
        Err(e) => {
            info!("{}",e.as_str());
            return;
        },
    };

    board.apply_move(bit_move);

    let mut updated_game = game.clone();
    updated_game.board_state = board.fen().to_string();
    updated_game.last_moved = (user_id, Utc::now().timestamp());

    databaselayer::set_game(&game).await;
    databaselayer::publish_update(&game).await;

}

fn construct_bit_move(parsed_move: EventData, board: &Board) -> Result<BitMove, String> {
    let from = parsed_move.from; //handle better
    let to = parsed_move.to;
    let flags: MoveFlag = match parsed_move.flags.unwrap().as_str() { //handle unwrap better
        "n" => MoveFlag::QuietMove,
        "c" => MoveFlag::Capture { ep_capture: false },
        "b" => MoveFlag::DoublePawnPush,
        "np" => MoveFlag::Promotion {
            capture: parsed_move.captured.is_some(),
            prom: piece_type_from_str(parsed_move.promotion.unwrap().as_str())
        },
        "k" => MoveFlag::Castle { king_side: true },
        "q" => MoveFlag::Castle { king_side: false },
        "e" => MoveFlag::Capture { ep_capture: true },
        _ => MoveFlag::QuietMove,  // Fallback if no match
    };
    if !(from.is_some() && to.is_some()) {
        info!("Invalid Moves: {:?}, {:?}", from, to);
        return Err("Invalid Move Data".to_string());
    }
    let info: PreMoveInfo = PreMoveInfo {
        src: SQ(square_to_index(from.unwrap().as_str()).unwrap()),
        dst: SQ(square_to_index(to.unwrap().as_str()).unwrap()),
        flags,
    };
    
    let bmove: BitMove = BitMove::init(info);
    if board.generate_moves().into_iter().collect::<Vec<BitMove>>().contains(&bmove) {
        return Ok(bmove);
    }
    else {
        info!("move is not valid!");
        return Err("Invalid Move".to_string());
    }
}

fn handle_surrender(user_id: u32, game_id: u32) {
}

fn handle_offer_draw(user_id: u32, game_id: u32) {
}

fn handle_accept_draw(user_id: u32, game_id: u32) {
}

fn handle_decline_draw(user_id: u32, game_id: u32) {
}

fn handle_send_reminder(user_id: u32, game_id: u32) {
}

async fn message_sender(sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, user_id: u32, game_id: u32) {
    // let mut sender = sender.lock().await;
    // let _ = sender.send(Message::Text(format!("Successfully authenticated user: {}", user_id))).await;
    // if let Some(game_data) = databaselayer::get_game(game_id).await {
    //     //do stuff
    //     let _ = sender.send(Message::Text(format!("game data: {:?}", game_data))).await;
    //     //looks like: game data: {"player_black": "3", "game_initiated": "0", "last_moved": "3 1729519142", "player_white": "2", "game_created": "1729519142"}

    // }
    let mut con = match databaselayer::redis_connection().await {
        Ok(c) => c,
        Err((_status, _json)) => {
            println!("Failed connecting to redis! (get game)");
            return; //handle this properly
        },
    };
    let channel_name = format!("game_updates:{}", game_id);
    let mut pubsub = con.subscribe(&channel_name).await.unwrap();

    loop {
        // Wait for a message
        let msg: redis::Msg = pubsub.get_message().await?;
        let payload: String = msg.get_payload()?;

        // Deserialize the JSON payload into a Game struct
        let game: Game = serde_json::from_str(&payload)?;
        println!("Received game update: {:?}", game);
        //lock sender
        //send fen, to, from, flag, whose turn it is to client
    }


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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EventMessage {
    event: String, //eg: game_start, game_move, game_surrender
    data: EventData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EventData {
    player: PlayerColour,
    from: Option<String>,
    to: Option<String>,
    flags: Option<String>,
    captured: Option<String>, //give the captured piece here, these 2 are optionals, only if there is a promotion (need to know if its a capturing promotion and to what piece)
    promotion: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
enum PlayerColour {
    White,
    Black,
}

// impl Not for PlayerColour {
//     type Output = PlayerColour;

//     fn not(self) -> PlayerColour {
//         match self {
//             PlayerColour::White => PlayerColour::Black,
//             PlayerColour::Black => PlayerColour::White,
//         }
//     }
// }

fn piece_type_from_str(piece_str: &str) -> PieceType {
    match piece_str {
        "p" => PieceType::P,
        "n" => PieceType::N,
        "b" => PieceType::B,
        "r" => PieceType::R,
        "q" => PieceType::Q,
        "k" => PieceType::K,
        _ => PieceType::None, // Default if no match found
    }
}

fn square_to_index(square: &str) -> Option<u8> {
    if square.len() != 2 {
        return None;
    }
    let file = square.chars().nth(0)?;
    let rank = square.chars().nth(1)?.to_digit(10)?;
    let file_index = (file as u8).checked_sub('a' as u8)?;
    let rank_index = (rank as u8).checked_sub(1)?;
    Some(rank_index * 8 + file_index)
}

