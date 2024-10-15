use axum::{
    extract::ws::{WebSocketUpgrade, Message, WebSocket},
    response::IntoResponse,
};

use crate::utils::decode_user_id;
use crate::utils::user_id_to_game_id;

pub async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) { //also takes the token here
    //PSUEDO-CODE:

    //let user_id: usize = decode_user_id(JWT)
    //let client = redis::Client::open("redis://127.0.0.1/").unwrap();

    // let (mut ws_sender, mut ws_receiver) = ws_stream.split();

	// let message_task = tokio::spawn(async {
	// 	while let Some(message) = ws_receiver.next().await {
	// 		match message { Ok(Message::Text(text)) => {
	// 			handle_messgae(text);
	// 		}
	// 	}
	// });

    // let game_update_task = tokio::spawn(async {
	// 	let mut pubsub = client
	// 		.get_async_connection()
	// 		.await.unwrap().into_pubsub();

    //     loop {
    //         match pubsub.on_message().await {
    //             Ok(message) => {
    //                 message_payload = messgage.getPayload().unwrap()
    //                 ws_sender.send(Message::Text(msg_payload)).await.unwrap();
    //             }
    //             Err(e) => {
    //                 break;
	// 		}
	// 	}
	// });
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

