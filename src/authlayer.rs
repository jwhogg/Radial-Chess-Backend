use axum::{
    response::IntoResponse,
    middleware::Next,
    http::{StatusCode, Request},
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm, TokenData};
use reqwest;
use serde::{Deserialize, Serialize};
use std::env; // To access environment variables
use dotenv::dotenv; // For loading .env variables

use crate::databaselayer;

#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    sub: String,  // User ID (subject)
    exp: usize,   // Expiration time
}

// Function to get JWKS (JSON Web Key Set)
async fn get_jwks(jwks_url: &str) -> Result<serde_json::Value, String> {
    let response = reqwest::get(jwks_url)
        .await
        .map_err(|_| "Failed to fetch JWKS".to_string())?;
    let jwks: serde_json::Value = response.json().await.map_err(|_| "Failed to parse JWKS".to_string())?;
    Ok(jwks)
}

// Function to validate the JWT token using the JWKS
pub async fn validate_token(token: &str) -> Result<TokenData<Claims>, String> {
    dotenv().ok();

    let jwks_url = match env::var("JWKS_URL") {
        Ok(url) => url,
        Err(_) => return Err("JWKS URL not configured".to_string()),
    };
    let jwks = get_jwks(&jwks_url).await?;

    let decoded_header = jsonwebtoken::decode_header(token)
        .map_err(|err| {
            eprintln!("Failed to decode the header: {}", err);
            "Failed to decode token header".to_string()
        })?;

    let kid = decoded_header.kid.unwrap();
    
    // Find the matching key in the JWKS
    let jwk = jwks["keys"]
        .as_array()
        .ok_or_else(|| "JWKS does not contain keys".to_string())?
        .iter()
        .find(|key| key["kid"] == kid)
        .ok_or_else(|| "Key not found in JWKS".to_string())?;

    // Extract the public key (RSA certificate) from the JWK
    let rsa_cert = jwk["x5c"][0]
        .as_str()
        .unwrap();

    let pem_cert = format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----",
        rsa_cert
    );

    // Parse the public key
    let key = DecodingKey::from_rsa_pem(pem_cert.as_bytes())
        .map_err(|err| {
            eprintln!("Failed to parse public key: {}", err);
            "Failed to parse public key".to_string()
        })?;

    // Decode and validate the JWT
    decode::<Claims>(
        token,
        &key,
        &Validation::new(Algorithm::RS256),
    )
    .map_err(|_| "Invalid token".to_string())
}

// Middleware to check for a valid 'sub' claim in the JWT token
pub async fn get_jwt_sub<B>(req: &Request<B>) -> Result<u32, (StatusCode, String)> {
    let token = match extract_bearer_token(req) {
        Some(token) => token,
        None => return Err((StatusCode::UNAUTHORIZED, "Authorization header missing or invalid".to_string())),
    };
    get_user_id_from_token(token).await
}

pub async fn get_user_id_from_token(token: &str) -> Result<u32, (StatusCode, String)> {

    match validate_token(token).await {
        Ok(token_data) => {
            //The subject will have the auth id (oath2 or auth0, eg: oath2|9231en290df193q10)
            if token_data.claims.sub.is_empty() {
                return Err((StatusCode::UNAUTHORIZED, "Missing 'sub' claim in token".to_string()));
            }

            //check if user exists in Users table, and make entry if not

            let external_user_id = token_data.claims.sub.as_str();

            let mut conn: mysql::PooledConn = match databaselayer::connect_to_db() {
                Ok(c) => c,
                Err(_) => return Err((StatusCode::INTERNAL_SERVER_ERROR, "Database connection failed".to_string())),
            };

            if let Ok(Some(user_id)) = databaselayer::get_user_id_by_external_user_id(&mut conn, external_user_id) {
                Ok(user_id)
            } else {
                let user_id = match databaselayer::create_user(&mut conn, external_user_id) {
                    Ok(id) => id,
                    Err(_) => return Err((StatusCode::NOT_FOUND, "User not found after creating user".to_string())),
                };
                
                Ok(user_id)
            }
        }
        Err(_) => {
            Err((StatusCode::UNAUTHORIZED, "Invalid or expired token".to_string()))
        }
    }
}

// Helper function to extract the Bearer token from the Authorization header
fn extract_bearer_token<B>(req: &Request<B>) -> Option<&str> {
    req.headers()
        .get("Authorization")?
        .to_str().ok()?
        .strip_prefix("Bearer ")
}

pub async fn user_id_from_request<B>(req: &Request<B>) -> Option<u32> {
    let token = extract_bearer_token(req);
    if token.is_some() {
        return get_user_id_from_token(token.unwrap()).await.ok();
    } else {
        return None;
    }
}

//auth middleware
pub async fn validate_jwt_sub<B>(req: Request<B>, next: Next<B>) -> impl IntoResponse {
    match get_jwt_sub(&req).await {
        Ok(_id) => {
            next.run(req).await
        }
        Err((status, message)) => {
            (status, message).into_response()
        }
    }
}
