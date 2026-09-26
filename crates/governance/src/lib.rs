//! CRYPTEK VIGIL — Governance Seal Verification
//!
//! BLAKE3-sealed governance decisions with replay protection.
//! Inspired by SNAPKITTYWEST/treasury-bridge ZK proof patterns.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceSeal {
    pub decision_id: String,
    pub proposal: String,
    pub action: String,
    pub seal_hash: String,
    pub sealed_at: DateTime<Utc>,
    pub quorum: u32,
    pub quorum_required: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyAction {
    LumpSum { symbol: String, amount_usd: f64 },
    LinearStream { symbol: String, total_usd: f64, months: u32 },
    BountyEscrow { symbol: String, amount_usd: f64, condition: String },
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    pub seal_valid: bool,
    pub quorum_met: bool,
    pub is_replay: bool,
    pub action: Option<PolicyAction>,
    pub reason: String,
}

pub fn compute_seal(decision_id: &str, proposal: &str, action: &str, timestamp: &DateTime<Utc>) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(decision_id.as_bytes());
    hasher.update(b"|");
    hasher.update(proposal.as_bytes());
    hasher.update(b"|");
    hasher.update(action.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.to_rfc3339().as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn seal_decision(
    decision_id: &str,
    proposal: &str,
    action: &str,
    quorum: u32,
    quorum_required: u32,
) -> GovernanceSeal {
    let now = Utc::now();
    let seal_hash = compute_seal(decision_id, proposal, action, &now);
    GovernanceSeal {
        decision_id: decision_id.to_string(),
        proposal: proposal.to_string(),
        action: action.to_string(),
        seal_hash,
        sealed_at: now,
        quorum,
        quorum_required,
    }
}

pub fn verify_seal(
    seal: &GovernanceSeal,
    seen_ids: &HashSet<String>,
) -> VerifyResult {
    if seen_ids.contains(&seal.decision_id) {
        return VerifyResult {
            seal_valid: false, quorum_met: false, is_replay: true,
            action: None, reason: "Replay: decision_id already processed".to_string(),
        };
    }
    let expected = compute_seal(&seal.decision_id, &seal.proposal, &seal.action, &seal.sealed_at);
    if expected != seal.seal_hash {
        return VerifyResult {
            seal_valid: false, quorum_met: false, is_replay: false,
            action: None, reason: "Seal hash mismatch — tampered or malformed".to_string(),
        };
    }
    let quorum_met = seal.quorum >= seal.quorum_required;
    if !quorum_met {
        return VerifyResult {
            seal_valid: true, quorum_met: false, is_replay: false,
            action: None,
            reason: format!("Quorum not met: {}/{} required", seal.quorum, seal.quorum_required),
        };
    }
    let policy = match seal.action.as_str() {
        "approve" => Some(PolicyAction::LumpSum {
            symbol: "PORTFOLIO".to_string(),
            amount_usd: 0.0,
        }),
        "reject" => Some(PolicyAction::Rejected),
        "defer" => None,
        _ => None,
    };
    VerifyResult {
        seal_valid: true, quorum_met: true, is_replay: false,
        action: policy,
        reason: "Seal verified, quorum met".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_and_verify() {
        let seal = seal_decision("GOV-001", "Increase XRP allocation by 10%", "approve", 3, 2);
        let seen = HashSet::new();
        let result = verify_seal(&seal, &seen);
        assert!(result.seal_valid);
        assert!(result.quorum_met);
        assert!(!result.is_replay);
    }

    #[test]
    fn test_tampered_seal() {
        let mut seal = seal_decision("GOV-002", "Sell all DOGE", "approve", 3, 2);
        seal.proposal = "Buy all DOGE".to_string();
        let seen = HashSet::new();
        let result = verify_seal(&seal, &seen);
        assert!(!result.seal_valid);
    }

    #[test]
    fn test_replay_protection() {
        let seal = seal_decision("GOV-003", "Add SOL position", "approve", 3, 2);
        let mut seen = HashSet::new();
        seen.insert("GOV-003".to_string());
        let result = verify_seal(&seal, &seen);
        assert!(result.is_replay);
        assert!(!result.seal_valid);
    }

    #[test]
    fn test_quorum_not_met() {
        let seal = seal_decision("GOV-004", "Rebalance", "approve", 1, 3);
        let seen = HashSet::new();
        let result = verify_seal(&seal, &seen);
        assert!(result.seal_valid);
        assert!(!result.quorum_met);
    }

    #[test]
    fn test_rejected_action() {
        let seal = seal_decision("GOV-005", "Bad idea", "reject", 3, 2);
        let seen = HashSet::new();
        let result = verify_seal(&seal, &seen);
        assert!(result.seal_valid);
        assert!(result.quorum_met);
        match result.action {
            Some(PolicyAction::Rejected) => {}
            _ => panic!("Expected Rejected action"),
        }
    }

    #[test]
    fn test_seal_deterministic() {
        let now = Utc::now();
        let h1 = compute_seal("X", "Y", "Z", &now);
        let h2 = compute_seal("X", "Y", "Z", &now);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }
}
