use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use redis::Client;
use redis::Connection;
use redis::ToRedisArgs;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use dotenv::dotenv;
use crate::gameserver::Game;

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

    pub async fn get_game(&self, game_id: u32) -> Option<Game> {
        let mut con = self.connection.lock().await;
        let game_string: String = con.hgetall(&format!("game:{}", game_id)).await.expect("Failed getting game");
        let game_json: Game = serde_json::from_str(&game_string).expect("failed deserialising game");
        Some(game_json)
    }

    pub async fn hget(&self, key: &str, field: &str) -> Option<String> {
        let mut con = self.connection.lock().await;
        con.hget(key, field).await.expect("Failed getting value")
    }

    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.hset(key, field, value).await
    }

    pub async fn hset_multiple<T: ToRedisArgs + Send + Sync>(&self, key: &str, fields: &[(String, T)]) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        for (field, value) in fields {
            con.hset(key, field, value).await?;
        }
        Ok(())
    }

    pub async fn publish(&self, channel: &str, message: &str) -> Result<(), redis::RedisError> {
        let mut con = self.connection.lock().await;
        con.publish(channel, message).await
    }

    //cannot subscribe using the multiplexed async con, and cannot deref con if we make it here
    //it is left upto the function caller to do con.as_pubsub() and the later logic for subscribing
    pub fn con_for_subscribe(&self) -> Connection {
        dotenv().ok();
        let redis_url = env::var("REDIS_URL").unwrap();
        let client = Client::open(redis_url).expect("Invalid Redis URL");
        let con = client.get_connection().expect("Failed to connect to Redis");
        con
        // let mut pubsub: redis::PubSub<'_> = con.as_pubsub();
        // pubsub.subscribe(channel).expect("failed subscribing to channel");
    }
}