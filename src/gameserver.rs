use std::{env, sync::Arc};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{sync::Mutex, task};
use crate::{authlayer, redislayer::RedisLayer, utils::decode_user_id};
use crate::utils::user_id_to_game_id;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use log::info;
use redis::{PubSub, ToRedisArgs};
// extern crate pleco;
use pleco;
use pleco::{core::piece_move::{MoveFlag, PreMoveInfo}, BitMove, Board, PieceType, SQ};
use dotenv::dotenv;
use tokio::sync::mpsc::{self, Sender};

// A game server to handle the game state when connecting over WebSocket to a single user
pub struct GameServer {
    redis_layer: Arc<Mutex<RedisLayer>>,
    game_id: u32,
    user_id: u32,
}

impl GameServer {
    pub async fn new(game_id: u32, user_id: u32) -> Self {
        let redis_layer = RedisLayer::new().await;

        GameServer {
            redis_layer: Arc::new(Mutex::new(redis_layer)),
            game_id: game_id,
            user_id: user_id,
        }
    }

    pub async fn handle_received_message(&self, msg: String) {
        info!("message from client {}", msg);
        let parsed_message: EventMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                info!("Failed to parse JSON");
                return;
            },
        };
    
        match parsed_message.event.as_str() {
            "game_move" => self.handle_move(parsed_message.data).await,
            "game_surrender" => handle_surrender(),
            "game_offer_draw" => handle_offer_draw(),
            "game_accept_draw" => handle_accept_draw(),
            "game_decline_draw" => handle_decline_draw(),
            "game_reminder" => handle_send_reminder(),
            _ => (),
        }
    }

    async fn handle_move(&self, data: EventData) {
        let redis_layer = self.redis_layer.lock().await;
        let game = match redis_layer.get_game(self.game_id).await {
            Some(game) => game,
            None => {
                info!("Failed to retreive game (handle_received_message)");
                return;
            },
        };
    
        if game.last_moved.0 == self.user_id {
            info!("Invalid! Player has already taken their turn");
            return;
        }
    
        let mut board: Board = Board::from_fen(&game.board_state).expect("Failed to load board state");
    
        let data = Arc::new(data);
    
        let bit_move = match construct_bit_move(data.clone(), &board) {
            Ok(bmove) => bmove,
            Err(e) => {
                info!("{}",e.as_str());
                return;
            },
        };
    
        board.apply_move(bit_move);
    
        let previous_move = Move {
            from: bit_move.get_src().to_string(),
            to: bit_move.get_dest().to_string(),
            flags: data.this_move.flags.clone(),
            captured: if data.this_move.captured.is_some() {data.this_move.captured.clone()} else {None},
            promotion: if data.this_move.promotion.is_some() {data.this_move.promotion.clone()} else {None},
        };

        let fields = vec![
            ("board_state".to_string(), board.fen()),
            ("last_moved".to_string(), serde_json::to_string(&(self.user_id, Utc::now().timestamp())).unwrap()),
            ("previous_move".to_string(), serde_json::to_string(&previous_move).unwrap()),
        ];

        if let Err(e) = redis_layer.hset_multiple(&format!("game:{}", game.game_id), &fields).await {
            info!("Error setting game info: {}", e);
            return;
        }
    
        let _ = redis_layer.publish(&format!("game_updates:{}", game.game_id), &format!("move:new:{}", self.user_id)).await;
    
    }

}


fn construct_bit_move(parsed_move: Arc<EventData>, board: &Board) -> Result<BitMove, String> {
    let parsed_move = &parsed_move.this_move;
    let from = &parsed_move.from; //handle better
    let to = &parsed_move.to;
    let flags: MoveFlag = match parsed_move.flags.as_str() { //handle unwrap better
        "n" => MoveFlag::QuietMove,
        "c" => MoveFlag::Capture { ep_capture: false },
        "b" => MoveFlag::DoublePawnPush,
        "np" => MoveFlag::Promotion {
            capture: parsed_move.captured.is_some(),
            prom: piece_type_from_str(parsed_move.promotion.as_deref().unwrap())
        },
        "k" => MoveFlag::Castle { king_side: true },
        "q" => MoveFlag::Castle { king_side: false },
        "e" => MoveFlag::Capture { ep_capture: true },
        _ => MoveFlag::QuietMove,  // Fallback if no match
    };

    let info: PreMoveInfo = PreMoveInfo {
        src: SQ(square_to_index(from).unwrap()),
        dst: SQ(square_to_index(to).unwrap()),
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

fn handle_surrender() {
}

fn handle_offer_draw() {
}

fn handle_accept_draw() {
}

fn handle_decline_draw() {
}

fn handle_send_reminder() {
}

pub async fn message_sender(sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, user_id: u32, game_id: u32) {
    //TODO: add functionality for relaying additional types of message
    let redislayer = RedisLayer::new().await;
    let channel = &format!("game_updates:{}", game_id);

    let mut c = redislayer.con_for_subscribe();
    let mut sub = c.as_pubsub();
    sub.subscribe(channel).expect("failed subscribing to channel");

    let game = redislayer.get_game(game_id).await.expect("failed to get game");
    let player_colour = if game.player_white == user_id {"white"} else {"black"};

    //send game_initated messge to client:
    {   
        info!("sending game_initiated message...");
        let message = Message::Text(serde_json::to_string(&json!({
            "event": "game_initiated",
            "playercolour": player_colour
        })).unwrap());
        let mut sender = sender.lock().await;
        let _ = sender.send(message).await;
    }

    loop {
        //expect messages to be "in:this:form", we want something like "move:new:{user_id}"
        let msg = sub.get_message().expect("Failed to receive message"); //get message is a blocking action
        let payload: String = msg.get_payload().expect("Failed to get payload");
        let parts: Vec<&str> = payload.split(':').collect();
        if parts.len() == 3
            && parts[0] == "move"
            && parts[1] == "new"
            && parts[2].parse().unwrap_or(0) != user_id
        {
            let game = redislayer.get_game(game_id).await.expect("failed to get game");
            let message = EventMessage {
                event: "game_move".to_string(),
                data: EventData {
                    player: if game.player_white == user_id {PlayerColour::White} else {PlayerColour::Black},
                    this_move: game.previous_move.unwrap(),
                    status: EventStatus::UpdateNewMove,
                }
            };

            let message = Message::Text(serde_json::to_string(&message).unwrap());

            let mut sender = sender.lock().await;
            let _ = sender.send(message).await;
        }
    }
}

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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EventMessage {
    event: String, //eg: game_start, game_move, game_surrender
    data: EventData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct EventData {
    player: PlayerColour,
    this_move: Move, //cant use 'move' word as it is reserved
    status: EventStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub game_id: u32,
    pub player_white: u32,
    pub player_black: u32,
    pub game_created: i64, //timestamp
    pub game_initiated: i64,
    pub last_moved: (u32, i64), // (user_id, timestamp)
    pub board_state: String,
    pub previous_move: Option<Move>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Move {
    pub from: String,
    pub to: String,
    pub flags: String,
    pub captured: Option<String>,
    pub promotion: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum PlayerColour {
    White,
    Black,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum EventStatus {
    EchoSuccess, //after the client makes a move, and the server validates it, send the new game state back with this status
    EchoFailure, //after the client makes a move, and the server INVALIDATES it, send the unchanged game state back with this status
    UpdateNewMove, //after the opponent makes a move, (which has been validated), send the new game state back with this status
    Reminder, //if the client asks to be re-sent the game state, send it along with this status
    ClientMessage,
}