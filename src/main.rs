use futures::stream::SplitSink;
use pleco::Player;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message, WebSocketStream, tungstenite::Error};
use futures::{StreamExt, SinkExt};
use std::env;
use std::net::SocketAddr;
use log::{info, error};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use serde;
use std::ops::Not;

extern crate pleco;
use pleco::{core::piece_move::{MoveFlag, PreMoveInfo}, BitMove, Board, PieceType, SQ};

const DISPLAY_BOARD: bool = true;
const HOST_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();

    // Get the address to bind to
    let addr = env::args().nth(1).unwrap_or_else(|| HOST_ADDR.to_string());
    let addr: SocketAddr = addr.parse().expect("Invalid address");

    // Create the TCP listener
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");

    info!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        // Spawn a new task for each connection
        tokio::spawn(handle_connection(stream));
    }
}

async fn handle_connection(stream: TcpStream) {
    let mut board = Board::start_pos();

    // Accept the WebSocket connection
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            info!("Error during the websocket handshake: {}", e);
            return;
        }
    };

    info!("Connection Established!");

    if DISPLAY_BOARD {
        println!("{}", board);
    }

    let (mut sender, mut receiver) = ws_stream.split();

    // Handle INCOMING messages
    while let Some(msg) = receiver.next().await {
        if let Err(e) = process_message(msg, &mut sender, &mut board).await {
            info!("Error processing message: {:?}", e);
            break;
        }
    }

    info!("Connection closed.");
}


async fn process_message(
    msg: Result<Message, Error>, 
    sender: &mut SplitSink<WebSocketStream<TcpStream>, Message>, 
    board: &mut Board
) -> Result<(), String> {
    
    match msg {
        Ok(Message::Text(text)) => {
            handle_message(text, board, sender).await?;
        }
        Ok(Message::Ping(ping)) => {
            sender.send(Message::Pong(ping))
                .await.map_err(
                    |e: tokio_tungstenite::tungstenite::Error|
                    format!("Failed to send pong: {:?}", e)
                )?;
        }
        Ok(Message::Close(reason)) => {
            // Handle closure by responding with a close frame
            sender.send(Message::Close(reason))
            .await.map_err(
                |e: tokio_tungstenite::tungstenite::Error|
                format!("Failed to send close frame: {:?}", e)
            )?;
            return Err("Connection closed".to_string());
        }
        Ok(_) => {},
        Err(e) => {
            return Err(format!("WebSocket error: {:?}", e));
        }
    }
    Ok(())
}

/// Handles incoming text messages, decodes them, and applies the move to the board.
async fn handle_message(msg: String, board: &mut Board, sender: &mut SplitSink<WebSocketStream<TcpStream>, Message>) -> Result<(), String> {

    let parsed_message: EventMessage = match serde_json::from_str(&msg) {
        Ok(m) => m,
        Err(e) => return Err(format!("Failed to parse JSON: {}", e)),
    };

    let response:  Result<EventMessage, String> = match parsed_message.event.as_str() {
        "game_start" => handle_game_start(parsed_message.data),
        "game_move" => handle_game_move(parsed_message.data),
        _ => Err("Unknown event".to_string()),
    };

    let response = match response {
        Ok(res) => res,
        Err(e) => return Err(e), 
    };

    let response_json = match serde_json::to_string(&response) {
        Ok(r) => r,
        Err(e) => return Err(format!("Failed to serialize response: {}", e)),
    };

    sender.send(Message::Text(response_json)).await.map_err(|e| format!("Failed to send response: {}", e))?;


    Ok(()) //remove

    // let player_move = decode_move(msg, board)?;
    // board.apply_move(player_move);

    // if DISPLAY_BOARD {
    //     println!("{}", board);
    // }

    // Ok(())
}

fn handle_game_start(data: EventData) -> Result<EventMessage, String> {
    let response = EventMessage {
        event: "game_start".to_string(),
        data: EventData {
            player: ! data.player,
            from: None,
            to: None,
            flags: None,
            status: Some("success".to_string()),

        },
    };

    return Ok(response);
}

fn handle_game_move(data: EventData, board: &mut Board) -> Result<EventMessage, String>{
    let player_move = decode_move(data, board)?;
    board.apply_move(player_move);

    if DISPLAY_BOARD {
        println!("{}", board);
    }
}

/// Decodes a move from the incoming message and constructs a BitMove.
fn decode_move(msg: EventData, board: &Board) -> Result<BitMove, String> {
    // match serde_json::from_str::<Value>(&msg) {
    //     Ok(parsed_move) => construct_bit_move(parsed_move, board),
    //     Err(e) => Err(format!("Failed to parse JSON: {}", e)),
    // }
    construct_bit_move(msg, board)
}


fn construct_bit_move(parsed_move: EventData, board: &Board) -> Result<BitMove, String> {
    let from = parsed_move["from"].as_str();
    let to = parsed_move["to"].as_str();

    let flags: MoveFlag = match parsed_move["flags"].as_str().unwrap() {
        "n" => MoveFlag::QuietMove,
        "c" => MoveFlag::Capture { ep_capture: false },
        "b" => MoveFlag::DoublePawnPush,
        "np" => MoveFlag::Promotion {
            capture: parsed_move["captured"].is_string(),
            prom: piece_type_from_str(parsed_move["promotion"].as_str().unwrap())
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
        src: SQ(square_to_index(from.unwrap()).unwrap()),
        dst: SQ(square_to_index(to.unwrap()).unwrap()),
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


// helper methods:
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
    status: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
enum PlayerColour {
    White,
    Black,
}

impl Not for PlayerColour {
    type Output = PlayerColour;

    fn not(self) -> PlayerColour {
        match self {
            PlayerColour::White => PlayerColour::Black,
            PlayerColour::Black => PlayerColour::White,
        }
    }
}