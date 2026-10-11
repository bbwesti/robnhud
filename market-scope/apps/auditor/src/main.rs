//! ═══════════════════════════════════════════════════════════════════════════════
//! MARKETSCOPE — WORM Chain Auditor
//!
//! Append-only audit trail. SHA-256 hash-chained. Tamper-evident.
//!
//! Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
//! ═══════════════════════════════════════════════════════════════════════════════

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single WORM chain block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormBlock {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub prev_hash: String,
    pub block_type: String,
    pub data: serde_json::Value,
    pub hash: String,
}

/// The WORM chain state.
#[derive(Debug)]
pub struct WormChain {
    pub blocks: Vec<WormBlock>,
}

impl WormChain {
    pub fn new() -> Self {
        let genesis = WormBlock {
            index: 0,
            timestamp: Utc::now(),
            prev_hash: String::new(),
            block_type: "genesis".to_string(),
            data: serde_json::json!({
                "platform": "MARKETSCOPE",
                "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058"
            }),
            hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        };

        Self {
            blocks: vec![genesis],
        }
    }

    /// Compute the hash of a block.
    fn compute_hash(index: u64, prev_hash: &str, block_type: &str, data: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        hasher.update(index.to_string().as_bytes());
        hasher.update(prev_hash.as_bytes());
        hasher.update(block_type.as_bytes());
        hasher.update(serde_json::to_vec(data).unwrap());
        format!("{:x}", hasher.finalize())
    }

    /// Append a new block to the chain.
    pub fn append(&mut self, block_type: &str, data: serde_json::Value) -> WormBlock {
        let index = self.blocks.len() as u64;
        let prev_hash = self.blocks.last().map(|b| b.hash.clone()).unwrap_or_default();
        let hash = Self::compute_hash(index, &prev_hash, block_type, &data);

        let block = WormBlock {
            index,
            timestamp: Utc::now(),
            prev_hash,
            block_type: block_type.to_string(),
            data,
            hash: hash.clone(),
        };

        self.blocks.push(block.clone());
        block
    }

    /// Verify the entire chain integrity.
    pub fn verify(&self) -> bool {
        for i in 1..self.blocks.len() {
            let block = &self.blocks[i];
            let prev = &self.blocks[i - 1];

            // Check prev_hash linkage
            if block.prev_hash != prev.hash {
                return false;
            }

            // Recompute hash
            let computed = Self::compute_hash(
                block.index,
                &block.prev_hash,
                &block.block_type,
                &block.data,
            );

            if computed != block.hash {
                return false;
            }
        }
        true
    }
}

/// Application state.
#[derive(Clone)]
pub struct AppState {
    pub chain: Arc<RwLock<WormChain>>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Health check.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "marketscope-auditor",
        "version": "0.1.0",
        "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058",
    }))
}

/// Append a new block to the WORM chain.
async fn append_block(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<WormBlock>, (StatusCode, String)> {
    let block_type = payload
        .get("block_type")
        .and_then(|v| v.as_str())
        .unwrap_or("data");

    let mut chain = state.chain.write().await;
    let block = chain.append(block_type, payload);

    Ok(Json(block))
}

/// Verify the chain integrity.
async fn verify_chain(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let chain = state.chain.read().await;
    let valid = chain.verify();
    let block_count = chain.blocks.len();

    Json(serde_json::json!({
        "valid": valid,
        "block_count": block_count,
        "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058",
    }))
}

/// Get the last N blocks.
async fn get_blocks(
    State(state): State<AppState>,
    axum::extract::Path(n): axum::extract::Path<usize>,
) -> Json<serde_json::Value> {
    let chain = state.chain.read().await;
    let start = chain.blocks.len().saturating_sub(n);
    let blocks: Vec<&WormBlock> = chain.blocks[start..].iter().collect();

    Json(serde_json::json!({
        "blocks": blocks,
        "total": chain.blocks.len(),
    }))
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = AppState {
        chain: Arc::new(RwLock::new(WormChain::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/append_block", post(append_block))
        .route("/verify", get(verify_chain))
        .route("/blocks/{n}", get(get_blocks))
        .with_state(state);

    let addr = "0.0.0.0:8090";
    tracing::info!("MARKETSCOPE auditor listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
