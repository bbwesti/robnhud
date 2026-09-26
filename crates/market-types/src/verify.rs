//! Ed25519 payload verification for market data.

use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPayload {
    pub data: String,
    pub data_hash: String,
    pub signature: String,
    pub pubkey: String,
    pub signed_at: chrono::DateTime<chrono::Utc>,
}

impl SignedPayload {
    pub fn sign<T: Serialize>(data: &T, signing_key: &SigningKey) -> Self {
        let json = serde_json::to_string(data).unwrap();
        let data_hash = {
            let mut hasher = Sha256::new();
            hasher.update(json.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let sig = signing_key.sign(data_hash.as_bytes());
        SignedPayload {
            data: json,
            data_hash,
            signature: hex::encode(sig.to_bytes()),
            pubkey: hex::encode(signing_key.verifying_key().to_bytes()),
            signed_at: chrono::Utc::now(),
        }
    }

    pub fn verify(&self) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(self.data.as_bytes());
        let computed_hash = format!("{:x}", hasher.finalize());
        if computed_hash != self.data_hash {
            return false;
        }
        let pk_bytes: [u8; 32] = match hex::decode(&self.pubkey) {
            Ok(b) => match b.try_into() {
                Ok(arr) => arr,
                Err(_) => return false,
            },
            Err(_) => return false,
        };
        let pk = match VerifyingKey::from_bytes(&pk_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig_bytes: [u8; 64] = match hex::decode(&self.signature) {
            Ok(b) => match b.try_into() {
                Ok(arr) => arr,
                Err(_) => return false,
            },
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_bytes);
        pk.verify(self.data_hash.as_bytes(), &sig).is_ok()
    }

    pub fn verify_and_deserialize<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        if !self.verify() { return None; }
        serde_json::from_str(&self.data).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OHLCV;

    fn test_key() -> SigningKey {
        let mut rng = rand::thread_rng();
        SigningKey::generate(&mut rng)
    }

    #[test]
    fn test_sign_and_verify() {
        let sk = test_key();
        let ohlcv = OHLCV {
            symbol: "XRP".into(), open: 0.58, high: 0.62,
            low: 0.56, close: 0.60, volume: 1_000_000.0,
            timestamp: chrono::Utc::now(),
        };
        let payload = SignedPayload::sign(&ohlcv, &sk);
        assert!(payload.verify());
    }

    #[test]
    fn test_tampered_data_fails() {
        let sk = test_key();
        let ohlcv = OHLCV {
            symbol: "XRP".into(), open: 0.58, high: 0.62,
            low: 0.56, close: 0.60, volume: 1_000_000.0,
            timestamp: chrono::Utc::now(),
        };
        let mut payload = SignedPayload::sign(&ohlcv, &sk);
        payload.data = payload.data.replace("XRP", "SCAM");
        assert!(!payload.verify());
    }

    #[test]
    fn test_verify_and_deserialize() {
        let sk = test_key();
        let ohlcv = OHLCV {
            symbol: "ETH".into(), open: 1800.0, high: 1850.0,
            low: 1780.0, close: 1820.0, volume: 50000.0,
            timestamp: chrono::Utc::now(),
        };
        let payload = SignedPayload::sign(&ohlcv, &sk);
        let recovered: OHLCV = payload.verify_and_deserialize().unwrap();
        assert_eq!(recovered.symbol, "ETH");
    }

    #[test]
    fn test_wrong_key_fails() {
        let sk1 = test_key();
        let sk2 = test_key();
        let ohlcv = OHLCV {
            symbol: "BTC".into(), open: 50000.0, high: 51000.0,
            low: 49000.0, close: 50500.0, volume: 5000.0,
            timestamp: chrono::Utc::now(),
        };
        let mut payload = SignedPayload::sign(&ohlcv, &sk1);
        payload.pubkey = hex::encode(sk2.verifying_key().to_bytes());
        assert!(!payload.verify());
    }
}
