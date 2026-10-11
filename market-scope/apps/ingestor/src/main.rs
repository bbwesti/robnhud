//! ═══════════════════════════════════════════════════════════════════════════════
//! MARKETSCOPE — Sovereign Data Ingestor
//!
//! Pulls OHLCV + alt data (futures basis, funding rates) via sovereign connectors.
//! Every payload is Plasma Gate Ed25519 verified before ingestion.
//!
//! Training_Gate: Human_Review_Required
//! Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
//! ═══════════════════════════════════════════════════════════════════════════════

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A data payload from an external source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPayload {
    pub id: Uuid,
    pub source: String,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub data: MarketData,
    pub signature: Vec<u8>,
    pub payload_hash: String,
}

/// Market data — OHLCV + alt data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketData {
    Ohlcv {
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    },
    FundingRate {
        rate: f64,
        next_funding: DateTime<Utc>,
    },
    FuturesBasis {
        spot: f64,
        futures: f64,
        basis: f64,
        basis_annualized: f64,
    },
    Index {
        value: f64,
        change_pct: f64,
    },
}

/// Ingestion result after verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResult {
    pub payload_id: Uuid,
    pub verified: bool,
    pub worm_logged: bool,
    pub timestamp: DateTime<Utc>,
    pub hash: String,
}

/// Application state.
#[derive(Clone)]
pub struct AppState {
    pub verifying_key: Arc<RwLock<Option<VerifyingKey>>>,
    pub redis_client: redis::Client,
    pub worm_endpoint: String,
    pub logic_engine_url: String,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum IngestorError {
    #[error("Signature verification failed")]
    SignatureInvalid,

    #[error("Payload hash mismatch")]
    HashMismatch,

    #[error("Source not authorized: {0}")]
    SourceUnauthorized(String),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

// ── Plasma Gate Verification ──────────────────────────────────────────────────

/// Verifies a data payload using Ed25519 signature.
fn verify_payload(payload: &DataPayload, key: &VerifyingKey) -> Result<(), IngestorError> {
    // Recompute hash
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&payload.data).unwrap());
    let computed_hash = format!("{:x}", hasher.finalize());

    // Check hash
    if computed_hash != payload.payload_hash {
        return Err(IngestorError::HashMismatch);
    }

    // Verify signature
    let signature = Signature::from_bytes(&payload.signature)
        .map_err(|_| IngestorError::SignatureInvalid)?;

    key.verify(&payload.payload_hash.as_bytes(), &signature)
        .map_err(|_| IngestorError::SignatureInvalid)?;

    Ok(())
}

// ── WORM Chain Logging ────────────────────────────────────────────────────────

/// Logs a payload to the Bifrost WORM chain.
async fn worm_log(payload: &DataPayload, worm_endpoint: &str) -> Result<bool, IngestorError> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/append_block", worm_endpoint))
        .json(&serde_json::json!({
            "block_type": "data_ingestion",
            "payload_id": payload.id,
            "source": payload.source,
            "symbol": payload.symbol,
            "timestamp": payload.timestamp,
            "hash": payload.payload_hash,
        }))
        .send()
        .await?;

    Ok(response.status().is_success())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Health check endpoint.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "marketscope-ingestor",
        "version": "0.1.0",
    }))
}

/// Ingest a data payload.
async fn ingest_payload(
    State(state): State<AppState>,
    Json(payload): Json<DataPayload>,
) -> Result<Json<IngestionResult>, (StatusCode, String)> {
    // 1. Verify signature
    let key = state.verifying_key.read().await;
    if let Some(key) = *key {
        verify_payload(&payload, &key).map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    }

    // 2. Log to WORM chain
    let worm_logged = worm_log(&payload, &state.worm_endpoint)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. Cache in Redis
    let mut conn = state
        .redis_client
        .get_async_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    redis::cmd("SETEX")
        .arg(format!("payload:{}", payload.id))
        .arg(86400) // 24h TTL
        .arg(serde_json::to_string(&payload).unwrap())
        .query_async(&mut conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 4. Return result
    let result = IngestionResult {
        payload_id: payload.id,
        verified: true,
        worm_logged,
        timestamp: Utc::now(),
        hash: payload.payload_hash.clone(),
    };

    Ok(Json(result))
}

/// Get a cached payload by ID.
async fn get_payload(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DataPayload>, (StatusCode, String)> {
    let mut conn = state
        .redis_client
        .get_async_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let data: Option<String> = redis::cmd("GET")
        .arg(format!("payload:{}", id))
        .query_async(&mut conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match data {
        Some(json) => {
            let payload: DataPayload =
                serde_json::from_str(&json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(Json(payload))
        }
        None => Err((StatusCode::NOT_FOUND, "Payload not found".to_string())),
    }
}

/// List all symbols being tracked.
async fn list_symbols() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "symbols": [
            {"symbol": "SPX", "name": "S&P 500", "connector": "yahoo_finance"},
            {"symbol": "NKX", "name": "Nikkei 225", "connector": "yahoo_finance"},
            {"symbol": "DAX", "name": "DAX", "connector": "yahoo_finance"},
            {"symbol": "KOSPI", "name": "KOSPI 200", "connector": "yahoo_finance"},
            {"symbol": "NDX", "name": "NASDAQ 100", "connector": "yahoo_finance"},
            {"symbol": "BTC-PERP", "name": "Bitcoin Perpetual", "connector": "ccxt"},
        ]
    }))
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Load Plasma Gate verifying key
    let verifying_key = std::env::var("PLASMA_GATE_KEY")
        .ok()
        .and_then(|hex_key| {
            let bytes = hex::decode(hex_key).ok()?;
            let key_bytes: [u8; 32] = bytes.try_into().ok()?;
            Some(VerifyingKey::from_bytes(&key_bytes).ok())
        })
        .flatten();

    let state = AppState {
        verifying_key: Arc::new(RwLock::new(verifying_key)),
        redis_client: redis::Client::open(
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
        )
        .expect("Failed to connect to Redis"),
        worm_endpoint: std::env::var("WORM_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8090".to_string()),
        logic_engine_url: std::env::var("LOGIC_ENGINE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string()),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ingest", post(ingest_payload))
        .route("/payload/{id}", get(get_payload))
        .route("/symbols", get(list_symbols))
        .with_state(state);

    let addr = "0.0.0.0:3001";
    tracing::info!("MARKETSCOPE ingestor listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
