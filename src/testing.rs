use axum::{response::{IntoResponse, Json}, http::{StatusCode, Request}};
use serde_json::json;
use crate::authlayer;

pub async fn test_setup(req: Request<hyper::Body>) -> impl IntoResponse {
    let response = match authlayer::get_jwt_sub(&req).await {
        Ok(id) => (StatusCode::OK, Json(json!({
            "message": "Function triggered successfully",
            "user_id": id //dont actually use this function, we dont want to expose id to the client
        }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "message": format!("Ran into problem getting the token from JWT: {:?}", e)
        })))
    };
    
    response
}