use std::{env, str::FromStr};

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
use sqlx::{
    migrate,
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode},
    MySqlPool,
};
use tokio::net::TcpListener;

fn connect_options(url: &str, ssl_ca: Option<String>) -> Result<MySqlConnectOptions> {
    let options = MySqlConnectOptions::from_str(url)?;

    let options = match ssl_ca {
        Some(path) => options.ssl_mode(MySqlSslMode::VerifyIdentity).ssl_ca(path),
        None if matches!(
            options.get_ssl_mode(),
            MySqlSslMode::VerifyCa | MySqlSslMode::VerifyIdentity
        ) =>
        {
            options
        }
        None => options.ssl_mode(MySqlSslMode::Required),
    };

    Ok(options)
}

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
    pk: PublicKey,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv()?;

    let ssl_ca = env::var("MYSQL_SSL_CA")
        .ok()
        .filter(|path| !path.is_empty());
    let options = connect_options(&env::var("DATABASE_URL")?, ssl_ca)?;
    let db = MySqlPoolOptions::new().connect_with(options).await?;

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

#[cfg(test)]
mod tests {
    use super::connect_options;
    use sqlx::mysql::MySqlSslMode;

    #[test]
    fn sqlx_does_not_enable_mysql_rsa() {
        let manifest = include_str!("../Cargo.toml");
        let sqlx_line = manifest
            .lines()
            .find(|line| line.starts_with("sqlx ="))
            .expect("sqlx dependency entry not found");
        assert!(
            !sqlx_line.contains("mysql-rsa"),
            "sqlx dependency should not enable the mysql-rsa feature"
        );
        assert!(
            sqlx_line.contains("tls-rustls"),
            "sqlx dependency should enable a rustls TLS backend"
        );
    }

    #[test]
    fn connection_requires_tls() {
        let options = connect_options("mysql://user:pw@localhost:3306/db?ssl-mode=disabled", None)
            .expect("failed to parse connection options");
        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::Required));

        let options = connect_options(
            "mysql://user:pw@localhost:3306/db?ssl-mode=disabled",
            Some("ca.pem".into()),
        )
        .expect("failed to parse connection options");
        assert!(matches!(
            options.get_ssl_mode(),
            MySqlSslMode::VerifyIdentity
        ));
    }

    #[test]
    fn keeps_verify_identity_without_ssl_ca() {
        let options = connect_options(
            "mysql://user:pw@localhost:3306/db?ssl-mode=verify_identity",
            None,
        )
        .expect("failed to parse connection options");
        assert!(matches!(
            options.get_ssl_mode(),
            MySqlSslMode::VerifyIdentity
        ));
    }

    #[test]
    fn keeps_verify_ca_without_ssl_ca() {
        let options = connect_options("mysql://user:pw@localhost:3306/db?ssl-mode=verify_ca", None)
            .expect("failed to parse connection options");
        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::VerifyCa));
    }

    #[test]
    fn disabled_without_ssl_ca_becomes_required() {
        let options = connect_options("mysql://user:pw@localhost:3306/db?ssl-mode=disabled", None)
            .expect("failed to parse connection options");
        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::Required));
    }

    #[test]
    fn preferred_without_ssl_ca_becomes_required() {
        let options = connect_options("mysql://user:pw@localhost:3306/db?ssl-mode=preferred", None)
            .expect("failed to parse connection options");
        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::Required));
    }

    #[test]
    fn unset_without_ssl_ca_becomes_required() {
        let options = connect_options("mysql://user:pw@localhost:3306/db", None)
            .expect("failed to parse connection options");
        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::Required));
    }

    #[test]
    fn ssl_ca_forces_verify_identity() {
        let options = connect_options(
            "mysql://user:pw@localhost:3306/db?ssl-mode=verify_ca",
            Some("ca.pem".into()),
        )
        .expect("failed to parse connection options");
        assert!(matches!(
            options.get_ssl_mode(),
            MySqlSslMode::VerifyIdentity
        ));
    }
}
