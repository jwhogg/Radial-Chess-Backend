use axum::{response::{IntoResponse, Json}, http::StatusCode};
use serde_json::json;

pub async fn test_setup() -> impl IntoResponse {
    let response = json!({
        "message": "Function triggered successfully",
    });
    (StatusCode::OK, Json(response))
}