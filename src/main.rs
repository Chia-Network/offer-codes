use std::env;

use anyhow::Result;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chia::{
    bls::{verify, PublicKey, Signature},
    protocol::{Bytes32, SpendBundle},
    traits::Streamable,
};
use chia_wallet_sdk::driver::Offer;
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::{migrate, MySqlPool};
use tokio::net::TcpListener;

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
    pk: PublicKey,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv()?;

    let db = MySqlPool::connect(&env::var("DATABASE_URL")?).await?;

    migrate!("./migrations").run(&db).await?;

    let pk = PublicKey::from_bytes(
        &hex::decode(env::var("PUBLIC_KEY")?)?
            .try_into()
            .expect("Invalid public key"),
    )?;

    let app = Router::new()
        .route("/upload_offer", post(upload_offer))
        .route("/download_offer", post(download_offer))
        .with_state(AppState { db, pk });

    let listener = TcpListener::bind(format!("0.0.0.0:{}", env::var("PORT")?)).await?;
    tracing::info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct UploadOffer {
    offer: String,
    signature: Signature,
}

#[derive(Debug, Serialize)]
struct UploadOfferResponse {
    code: Bytes32,
}

async fn upload_offer(
    State(state): State<AppState>,
    Json(req): Json<UploadOffer>,
) -> Result<Json<UploadOfferResponse>, StatusCode> {
    let spend_bundle: SpendBundle = Offer::decode(&req.offer)
        .map_err(|error| {
            tracing::error!("Error decoding offer: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .into();

    let code = Bytes32::new(spend_bundle.hash());

    if !verify(&req.signature, &state.pk, code) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(error) = sqlx::query!(
        "INSERT IGNORE INTO offers (code, offer) VALUES (?, ?)",
        code.to_vec(),
        spend_bundle.to_bytes().unwrap()
    )
    .execute(&state.db)
    .await
    {
        tracing::error!("Error inserting offer: {error}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(UploadOfferResponse { code }))
}

#[derive(Debug, Deserialize)]
struct DownloadOffer {
    code: Bytes32,
}

#[derive(Debug, Serialize)]
struct DownloadOfferResponse {
    offer: Option<String>,
}

async fn download_offer(
    State(state): State<AppState>,
    Json(req): Json<DownloadOffer>,
) -> Result<Json<DownloadOfferResponse>, StatusCode> {
    let offer = match sqlx::query!("SELECT offer FROM offers WHERE code = ?", req.code.to_vec())
        .fetch_optional(&state.db)
        .await
    {
        Ok(row) => row.map(|row| row.offer),
        Err(error) => {
            tracing::error!("Error fetching offer: {error}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let Some(offer) = offer else {
        return Ok(Json(DownloadOfferResponse { offer: None }));
    };

    let offer = Offer::from_bytes(&offer).map_err(|error| {
        tracing::error!("Error deserializing offer: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let offer = offer.encode().map_err(|error| {
        tracing::error!("Error encoding offer: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(DownloadOfferResponse { offer: Some(offer) }))
}
