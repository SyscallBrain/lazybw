//! SSH key generation and parsing for the SSH key item type.

use ssh_key::private::{Ed25519Keypair, KeypairData};
use ssh_key::{Algorithm, LineEnding, PrivateKey};

/// A generated key pair ready to store in an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedKey {
    /// OpenSSH private key in PEM form.
    pub private_key: String,
    /// OpenSSH public key line.
    pub public_key: String,
    /// `SHA256:…` fingerprint.
    pub fingerprint: String,
}

/// Generates an Ed25519 key pair with the given comment.
pub fn generate_ed25519(comment: &str) -> Result<GeneratedKey, ssh_key::Error> {
    use ssh_key::rand_core::OsRng;
    let keypair = Ed25519Keypair::random(&mut OsRng);
    let private = PrivateKey::new(KeypairData::Ed25519(keypair), comment)?;
    Ok(GeneratedKey {
        private_key: private.to_openssh(LineEnding::LF)?.to_string(),
        public_key: private.public_key().to_openssh()?,
        fingerprint: private.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    })
}

/// Parses an existing OpenSSH private key (no passphrase).
pub fn parse_private_key(pem: &str) -> Result<GeneratedKey, ssh_key::Error> {
    let private = PrivateKey::from_openssh(pem)?;
    Ok(GeneratedKey {
        private_key: private.to_openssh(LineEnding::LF)?.to_string(),
        public_key: private.public_key().to_openssh()?,
        fingerprint: private.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    })
}

/// Parses an existing OpenSSH private key, decrypting it with `passphrase`.
pub fn parse_private_key_with_passphrase(
    pem: &str,
    passphrase: &str,
) -> Result<GeneratedKey, ssh_key::Error> {
    let private = PrivateKey::from_openssh(pem)?;
    let decrypted = private.decrypt(passphrase)?;
    Ok(GeneratedKey {
        private_key: decrypted.to_openssh(LineEnding::LF)?.to_string(),
        public_key: decrypted.public_key().to_openssh()?,
        fingerprint: decrypted.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    })
}

/// True when a private key cannot be read without a passphrase.
#[must_use]
pub fn is_encrypted(pem: &str) -> bool {
    PrivateKey::from_openssh(pem).is_err() || pem.contains("ENCRYPTED")
}

/// The algorithm label used in the fingerprint line.
#[must_use]
pub fn algorithm_label(pem: &str) -> Option<String> {
    let key = PrivateKey::from_openssh(pem).ok()?;
    Some(match key.algorithm() {
        Algorithm::Ed25519 => "ED25519".to_owned(),
        Algorithm::Rsa { .. } => "RSA".to_owned(),
        Algorithm::Ecdsa { .. } => "ECDSA".to_owned(),
        Algorithm::Dsa => "DSA".to_owned(),
        other => format!("{other:?}").to_uppercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_parses_ed25519() {
        let key = generate_ed25519("lazybw-test").unwrap();
        assert!(key.private_key.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(key.public_key.starts_with("ssh-ed25519 "));
        assert!(key.public_key.ends_with("lazybw-test"));
        assert!(key.fingerprint.starts_with("SHA256:"));
        let reparsed = parse_private_key(&key.private_key).unwrap();
        assert_eq!(reparsed.public_key, key.public_key);
        assert_eq!(reparsed.fingerprint, key.fingerprint);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_private_key("nope").is_err());
    }

    #[test]
    fn labels_algorithm() {
        let key = generate_ed25519("x").unwrap();
        assert_eq!(algorithm_label(&key.private_key).as_deref(), Some("ED25519"));
    }
}
