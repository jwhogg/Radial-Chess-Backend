use crate::utils::decode_user_id;


pub async fn matchmaking_handler() { //takes in JWT
    // let user_id: int = decode_user_id(JWT);

    //adds user_id to redis matchmaking pool

    //returns HTTP 102
}

pub async fn bot_handler() {
 // TODO
}

pub async fn matchmaking_status() {
    //checks active game pool for user_id (maybe also task id, might not even need this)

    //if there is an active game, return 200 OK, user should switch to websocket

    //if no game, return HTTP 102
}

pub async fn match_maker() {
    //on loop:
        //BRPOP the matchmaking pool in redis, twice, getting a player each time

        //if both players are Some(), then remvoe from the pool (if BRPOP doesnt already do that?) and make a new game
        //  - Add the game to the Active Games collection in redis
}