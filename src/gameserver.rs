use std::{env, sync::Arc};
use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task};
use crate::{authlayer, databaselayer::{self, Game}, redislayer::RedisLayer, utils::decode_user_id};
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
    
        let previous_move = databaselayer::Move {
            from: bit_move.get_src().to_string(),
            to: bit_move.get_dest().to_string(),
            flags: data.flags.clone().unwrap(),
            captured: if data.captured.is_some() {data.captured.clone()} else {None},
            promotion: if data.promotion.is_some() {data.promotion.clone()} else {None},
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
    
        redis_layer.publish(&format!("game_updates:{}", game.game_id), &format!("move:new:{}", self.user_id)).await;
    
    }

}


fn construct_bit_move(parsed_move: Arc<EventData>, board: &Board) -> Result<BitMove, String> {
    let from = &parsed_move.from; //handle better
    let to = &parsed_move.to;
    let flags: MoveFlag = match parsed_move.flags.as_deref().unwrap() { //handle unwrap better
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
    if !(from.is_some() && to.is_some()) {
        info!("Invalid Moves: {:?}, {:?}", from, to);
        return Err("Invalid Move Data".to_string());
    }
    let info: PreMoveInfo = PreMoveInfo {
        src: SQ(square_to_index(from.as_deref().unwrap()).unwrap()),
        dst: SQ(square_to_index(to.as_deref().unwrap()).unwrap()),
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
    let mut sender = sender.lock().await;
    let _ = sender.send(Message::Text(("Testing from message sender".to_string())));
    // dotenv().ok();
    
    // let redis_url = env::var("REDIS_URL").unwrap();
    // let client = redis::Client::open(redis_url).unwrap();
    // let mut con = client.get_connection().unwrap();
    // let mut pubsub = con.as_pubsub();
    // let channel_name = format!("game_updates:{}", game_id);
    // pubsub.subscribe(channel_name).unwrap();

    // loop {
    //     let msg: redis::Msg = pubsub.get_message().unwrap(); //handle better
    //     let payload: String = msg.get_payload().unwrap();

    //     let game: Game = serde_json::from_str(&payload).unwrap();
    //     println!("Received game update: {:?}", game);
    //     // let mut sender = sender.lock().await;
    //     //send fen, to, from, flag, whose turn it is to client
    //     // let message= EventMessage {
    //     //     event: "game_update".to_string(),
    //     //     data: EventData {
    //     //         player: game.last_moved.0,
    //     //         from: game.previous_move.from,
    //     //     }
    //     // };
    // }


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
    from: Option<String>,
    to: Option<String>,
    flags: Option<String>,
    captured: Option<String>, //give the captured piece here, these 2 are optionals, only if there is a promotion (need to know if its a capturing promotion and to what piece)
    promotion: Option<String>,
    status: EventStatus,
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
}