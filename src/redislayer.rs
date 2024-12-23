use log::info;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use redis::Client;
use redis::Connection;
use redis::ToRedisArgs;
use redis_async::client::PubsubConnection;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use dotenv::dotenv;
use crate::gameserver::Game;
use redis::RedisResult;
use std::collections::HashMap;
use redis_async::client::pubsub::pubsub_connect;

#[derive(Clone)]
pub struct RedisLayer {
    connection: Arc<Mutex<MultiplexedConnection>>,
}

impl RedisLayer {
    pub async fn new() -> Self {
        let connection = RedisLayer::new_connection().await;
        
        RedisLayer {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    async fn new_connection() -> MultiplexedConnection {
        dotenv().ok();
        let redis_url = env::var("REDIS_URL").unwrap();
        let client = Client::open(redis_url).expect("Invalid Redis URL");
        client.get_multiplexed_async_connection().await.expect("Failed to connect to Redis")
    }

    //redis set (key, value) command
    pub async fn set(&self, key: &str, value: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.set(key, value).await
    }

    //redis get (key, value) command
    pub async fn get(&self, key: &str) -> Result<String, redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.get(key).await
    }

    pub async fn del(&self, key: &str) -> Result<String, redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.del(key).await
    }

    pub async fn zscore(&self, key: &str, member: &str) -> Result<Option<f64>, redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.zscore(key, member).await
    }

    pub async fn zadd(&self, key: &str, member: &str, score: f64) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.zadd(key, member, score).await
    }

    pub async fn zrem(&self, key: &str, member: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.zrem(key, member).await
    }    

    pub async fn zcard(&self, key: &str) -> Result<u64, redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.zcard(key).await
    }

    pub async fn zpopmin(&self, key: &str, count: isize) -> Result<Vec<(String, f64)>, redis::RedisError> {
        let mut con = self.connection.lock().await;
        let result: Vec<(String, f64)>  = con.zpopmin(key, count).await?;
        Ok(result)
    }

    pub async fn incr(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut con = self.connection.lock().await;
        let result: i64 = con.incr(key, 1).await?;
        Ok(result)
    }

    pub async fn get_game(&self, game_id: u32) -> Option<Game> {
        let mut con = self.connection.lock().await;
        let game_data: RedisResult<HashMap<String, String>> = con.hgetall(&format!("game:{}", game_id)).await;

        match game_data {
            Ok(data) => {
                let game = Game {
                    game_id: data.get("game_id")?.parse().ok().unwrap(),
                    player_white: data.get("player_white").unwrap().parse().ok().unwrap(),
                    player_black: data.get("player_black").unwrap().parse().ok().unwrap(),
                    game_created: data.get("game_created").unwrap().parse().ok().unwrap(),
                    game_initiated: data.get("game_initiated").unwrap().parse().ok().unwrap(),
                    last_moved: {
                        let last_moved_str = data.get("last_moved").unwrap();
                        let last_moved_tuple: (u32, i64) = serde_json::from_str(last_moved_str).ok().unwrap();
                        last_moved_tuple
                    },
                    board_state: data.get("board_state").unwrap().clone(),
                    previous_move: {
                        let previous_move_str = data.get("previous_move").unwrap();
                        serde_json::from_str(previous_move_str).ok().unwrap()
                    },
                };
                Some(game)
            },
            Err(_) => {
                None
            }
        }
    }

    pub async fn hget(&self, key: &str, field: &str) -> Option<String> {
        let mut con = self.connection.lock().await;
        con.hget(key, field).await.expect("Failed getting value")
    }

    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.hset(key, field, value).await
    }

    pub async fn hincr(&self, key: &str, field: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.hincr(key, field, 1).await
    }

    pub async fn hset_multiple<T: ToRedisArgs + Send + Sync>(&self, key: &str, fields: &[(String, T)]) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        for (field, value) in fields {
            con.hset(key, field, value).await?;
        }
        Ok(())
    }

    //for creating a game (ie adding it to redis)
    pub async fn hset_game(&self, game: &Game) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
    
        let fields = vec![
            ("game_id".to_string(), game.game_id.to_string()),
            ("player_white".to_string(), game.player_white.to_string()),
            ("player_black".to_string(), game.player_black.to_string()),
            ("game_created".to_string(), game.game_created.to_string()),
            ("game_initiated".to_string(), game.game_initiated.to_string()),
            ("last_moved".to_string(), serde_json::to_string(&game.last_moved).unwrap()), // Using Debug trait for tuple
            ("board_state".to_string(), game.board_state.clone()),
            ("previous_move".to_string(), serde_json::to_string(&game.previous_move).unwrap()), // Using Debug for Option
        ];
    
        con.hset_multiple(&format!("game:{}", game.game_id), &fields).await
    }

    pub async fn publish(&self, channel: &str, message: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.publish(channel, message).await
    }

    pub async fn get_pubsub(&self) -> PubsubConnection{
        // let redis_url = env::var("REDIS_URL").unwrap();
        let redis_url = "127.0.0.1".to_string(); //because this crate doesnt accept //redis...
        let pubsub = pubsub_connect(redis_url, 6379).await.unwrap();
        pubsub
    }
}