//! Ed25519 key management for signed WORM blocks.
//! Keys are stored as raw 32-byte files. Generated on first use.

use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use std::path::Path;

const SK_FILENAME: &str = "vigil_worm_signing.key";

/// Load existing signing key from `dir/vigil_worm_signing.key`,
/// or generate a new one and save it.
pub fn load_or_generate_signing_key(dir: &Path) -> Result<SigningKey, std::io::Error> {
    let key_path = dir.join(SK_FILENAME);
    if key_path.exists() {
        let bytes = std::fs::read(&key_path)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad key length"))?;
        Ok(SigningKey::from_bytes(&arr))
    } else {
        let mut rng = rand::thread_rng();
        let sk = SigningKey::generate(&mut rng);
        std::fs::write(&key_path, sk.to_bytes())?;
        Ok(sk)
    }
}

/// Sign a message, returning the Ed25519 signature.
pub fn sign_message(sk: &SigningKey, message: &[u8]) -> Signature {
    sk.sign(message)
}

/// Verify a signature against a public key and message.
pub fn verify_signature(pk: &VerifyingKey, message: &[u8], sig: &Signature) -> bool {
    pk.verify(message, sig).is_ok()
}

/// Encode a signature as hex string.
pub fn signature_to_hex(sig: &Signature) -> String {
    hex::encode(sig.to_bytes())
}

/// Decode a hex string to a Signature.
pub fn signature_from_hex(hex_str: &str) -> Option<Signature> {
    let bytes = hex::decode(hex_str).ok()?;
    let arr: [u8; 64] = bytes.try_into().ok()?;
    Some(Signature::from_bytes(&arr))
}

/// Encode a public key as hex string.
pub fn pubkey_to_hex(pk: &VerifyingKey) -> String {
    hex::encode(pk.to_bytes())
}

/// Decode a hex string to a VerifyingKey.
pub fn pubkey_from_hex(hex_str: &str) -> Option<VerifyingKey> {
    let bytes = hex::decode(hex_str).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_load_keypair() {
        let dir = std::env::temp_dir().join("vigil_test_keys_gen");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let sk = load_or_generate_signing_key(&dir).unwrap();

        // Second call should load the same key
        let sk2 = load_or_generate_signing_key(&dir).unwrap();
        assert_eq!(sk.to_bytes(), sk2.to_bytes());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_sign_and_verify() {
        let dir = std::env::temp_dir().join("vigil_test_keys_sign");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let sk = load_or_generate_signing_key(&dir).unwrap();
        let message = b"test block data";

        let sig = sign_message(&sk, message);
        assert!(verify_signature(&sk.verifying_key(), message, &sig));

        // Tampered message should fail
        assert!(!verify_signature(&sk.verifying_key(), b"tampered", &sig));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_hex_roundtrip() {
        let dir = std::env::temp_dir().join("vigil_test_keys_hex");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let sk = load_or_generate_signing_key(&dir).unwrap();
        let sig = sign_message(&sk, b"data");
        let hex_sig = signature_to_hex(&sig);
        let recovered = signature_from_hex(&hex_sig).unwrap();
        assert_eq!(sig.to_bytes(), recovered.to_bytes());

        let hex_pk = pubkey_to_hex(&sk.verifying_key());
        let recovered_pk = pubkey_from_hex(&hex_pk).unwrap();
        assert_eq!(sk.verifying_key().to_bytes(), recovered_pk.to_bytes());

        std::fs::remove_dir_all(&dir).ok();
    }
}
