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

/// Parses an existing OpenSSH private key (no passphrase). A key that has a
/// passphrase is refused (`Error::Encrypted`): its public half reads without
/// it, but the stored key would be unusable.
pub fn parse_private_key(pem: &str) -> Result<GeneratedKey, ssh_key::Error> {
    let private = PrivateKey::from_openssh(pem)?;
    if private.is_encrypted() {
        return Err(ssh_key::Error::Encrypted);
    }
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

/// True when `pem` is an OpenSSH private key that has a passphrase. Text
/// that is not a key at all is not "encrypted".
#[must_use]
pub fn is_encrypted(pem: &str) -> bool {
    PrivateKey::from_openssh(pem).is_ok_and(|key| key.is_encrypted())
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

    fn encrypted(passphrase: &str) -> (GeneratedKey, String) {
        use ssh_key::rand_core::OsRng;
        let plain = generate_ed25519("lazybw-test").unwrap();
        let pem = PrivateKey::from_openssh(&plain.private_key)
            .unwrap()
            .encrypt(&mut OsRng, passphrase)
            .unwrap()
            .to_openssh(LineEnding::LF)
            .unwrap()
            .to_string();
        (plain, pem)
    }

    /// The public half of an encrypted OpenSSH key reads without the
    /// passphrase; the key must not be taken as usable for that.
    #[test]
    fn an_encrypted_key_is_refused_without_its_passphrase() {
        let (plain, pem) = encrypted("hunter2");
        assert!(is_encrypted(&pem));
        assert!(parse_private_key(&pem).is_err());
        let opened = parse_private_key_with_passphrase(&pem, "hunter2").unwrap();
        assert_eq!(opened.fingerprint, plain.fingerprint);
        assert!(!opened.private_key.contains("ENCRYPTED"));
        assert!(!is_encrypted(&opened.private_key));
    }

    #[test]
    fn text_that_is_not_a_key_is_not_called_encrypted() {
        assert!(!is_encrypted("not a key"));
        assert!(!is_encrypted(""));
        assert!(!is_encrypted(&generate_ed25519("x").unwrap().private_key));
    }

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
