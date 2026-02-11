use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use std::fs;
use std::path::{Path, PathBuf};

const AES_NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;
const SALT_FILENAME: &str = "storage.salt";

/// Secure storage for API keys and sensitive data.
/// Uses AES-256-GCM encryption with a key derived via Argon2id from
/// machine-specific context and a persisted random salt.
pub struct SecureStorage {
    cipher: Aes256Gcm,
    key_material: [u8; 32],
}

impl SecureStorage {
    /// Create a new SecureStorage with a key derived via Argon2id.
    ///
    /// The salt is loaded from (or generated and saved to) `~/.hive/storage.salt`.
    pub fn new() -> Result<Self> {
        let salt_path = Self::default_salt_path()?;
        let key_material = Self::derive_key(&salt_path)?;
        Ok(Self::from_key_material(key_material))
    }

    /// Create a SecureStorage with a salt file at a custom path.
    /// Useful for testing without touching `~/.hive/`.
    pub fn with_salt_path(salt_path: &Path) -> Result<Self> {
        let key_material = Self::derive_key(salt_path)?;
        Ok(Self::from_key_material(key_material))
    }

    /// Create a duplicate instance reusing the same derived key material.
    /// Avoids re-running Argon2 key derivation.
    pub fn duplicate(&self) -> Self {
        Self::from_key_material(self.key_material)
    }

    fn from_key_material(key_material: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(&key_material);
        let cipher = Aes256Gcm::new(key);
        Self { cipher, key_material }
    }

    /// Encrypt a plaintext string, returning hex-encoded ciphertext.
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let nonce_bytes: [u8; AES_NONCE_LEN] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(hex::encode(result))
    }

    /// Decrypt a hex-encoded ciphertext string.
    pub fn decrypt(&self, hex_ciphertext: &str) -> Result<String> {
        let data = hex::decode(hex_ciphertext).context("Invalid hex")?;
        if data.len() < AES_NONCE_LEN {
            anyhow::bail!("Ciphertext too short");
        }

        let (nonce_bytes, ciphertext) = data.split_at(AES_NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

        String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")
    }

    /// Returns the default salt file path: `~/.hive/storage.salt`.
    fn default_salt_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".hive").join(SALT_FILENAME))
    }

    /// Load a salt from disk, or generate and persist a new one.
    fn load_or_create_salt(salt_path: &Path) -> Result<[u8; SALT_LEN]> {
        if let Ok(data) = fs::read(salt_path) {
            if data.len() == SALT_LEN {
                let mut salt = [0u8; SALT_LEN];
                salt.copy_from_slice(&data);
                return Ok(salt);
            }
            // Salt file exists but has wrong length -- regenerate.
        }

        let salt: [u8; SALT_LEN] = rand::random();

        // Ensure parent directory exists.
        if let Some(parent) = salt_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        fs::write(salt_path, salt)
            .with_context(|| format!("Failed to write salt file {}", salt_path.display()))?;

        Ok(salt)
    }

    /// Derive a 256-bit key using Argon2id with a persisted random salt and
    /// machine-specific context (username + home directory).
    ///
    /// Parameters: Argon2id, m=19456 KiB (~19 MB), t=2 iterations, p=1 lane.
    /// These are reasonable defaults that balance security and startup latency.
    fn derive_key(salt_path: &Path) -> Result<[u8; 32]> {
        let salt = Self::load_or_create_salt(salt_path)?;

        // Gather machine-specific input material.
        let username = whoami::username();
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let password = format!("hive-secure-storage-v2:{username}:{home}");

        let params = Params::new(19_456, 2, 1, Some(32))
            .map_err(|e| anyhow::anyhow!("Invalid Argon2 params: {e}"))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {e}"))?;

        Ok(key)
    }

    /// Derive a key from an explicit salt and password. Used in tests to verify
    /// determinism and independence without touching the filesystem.
    #[cfg(test)]
    fn derive_key_raw(password: &[u8], salt: &[u8; SALT_LEN]) -> Result<[u8; 32]> {
        let params = Params::new(19_456, 2, 1, Some(32))
            .map_err(|e| anyhow::anyhow!("Invalid Argon2 params: {e}"))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password, salt, &mut key)
            .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {e}"))?;

        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a SecureStorage whose salt lives in a temp directory.
    fn storage_in(dir: &Path) -> SecureStorage {
        let salt_path = dir.join(SALT_FILENAME);
        SecureStorage::with_salt_path(&salt_path).unwrap()
    }

    // ---- basic encrypt / decrypt (same as before) ----

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "sk-ant-api03-secret-key-12345";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_hex() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("test").unwrap();
        assert!(hex::decode(&encrypted).is_ok());
    }

    #[test]
    fn encrypt_different_each_time() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let enc1 = storage.encrypt("same input").unwrap();
        let enc2 = storage.encrypt("same input").unwrap();
        // Different nonces produce different ciphertexts
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn decrypt_invalid_hex() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let result = storage.decrypt("not-hex-zzz");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_too_short() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let result = storage.decrypt("aabb");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_tampered_ciphertext() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("secret").unwrap();
        let mut bytes = hex::decode(&encrypted).unwrap();
        if bytes.len() > 15 {
            bytes[15] ^= 0xff;
        }
        let tampered = hex::encode(bytes);
        let result = storage.decrypt(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_empty_string() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("").unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn encrypt_unicode() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "Hello \u{1f30d} \u{4e16}\u{754c} \u{0645}\u{0631}\u{062d}\u{0628}\u{0627}";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_long_string() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "x".repeat(10_000);
        let encrypted = storage.encrypt(&plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ---- Argon2 key derivation ----

    #[test]
    fn derive_key_deterministic_with_same_salt() {
        let salt: [u8; SALT_LEN] = [1u8; SALT_LEN];
        let password = b"test-password";
        let key1 = SecureStorage::derive_key_raw(password, &salt).unwrap();
        let key2 = SecureStorage::derive_key_raw(password, &salt).unwrap();
        assert_eq!(key1, key2, "Same salt + password must produce same key");
    }

    #[test]
    fn derive_key_different_salts_produce_different_keys() {
        let salt_a: [u8; SALT_LEN] = [1u8; SALT_LEN];
        let salt_b: [u8; SALT_LEN] = [2u8; SALT_LEN];
        let password = b"test-password";
        let key_a = SecureStorage::derive_key_raw(password, &salt_a).unwrap();
        let key_b = SecureStorage::derive_key_raw(password, &salt_b).unwrap();
        assert_ne!(key_a, key_b, "Different salts must produce different keys");
    }

    #[test]
    fn derive_key_different_passwords_produce_different_keys() {
        let salt: [u8; SALT_LEN] = [42u8; SALT_LEN];
        let key_a = SecureStorage::derive_key_raw(b"password-a", &salt).unwrap();
        let key_b = SecureStorage::derive_key_raw(b"password-b", &salt).unwrap();
        assert_ne!(key_a, key_b, "Different passwords must produce different keys");
    }

    #[test]
    fn derived_key_is_32_bytes() {
        let salt: [u8; SALT_LEN] = [7u8; SALT_LEN];
        let key = SecureStorage::derive_key_raw(b"any", &salt).unwrap();
        assert_eq!(key.len(), 32);
    }

    // ---- salt file management ----

    #[test]
    fn salt_file_created_on_first_use() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);
        assert!(!salt_path.exists());

        let _storage = SecureStorage::with_salt_path(&salt_path).unwrap();

        assert!(salt_path.exists());
        let data = fs::read(&salt_path).unwrap();
        assert_eq!(data.len(), SALT_LEN);
    }

    #[test]
    fn salt_file_reused_on_second_use() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);

        // First use creates the salt.
        let storage1 = SecureStorage::with_salt_path(&salt_path).unwrap();
        let salt_bytes_1 = fs::read(&salt_path).unwrap();

        // Encrypt something.
        let encrypted = storage1.encrypt("persisted secret").unwrap();

        // Second use loads the same salt; decryption must succeed.
        let storage2 = SecureStorage::with_salt_path(&salt_path).unwrap();
        let salt_bytes_2 = fs::read(&salt_path).unwrap();
        assert_eq!(salt_bytes_1, salt_bytes_2, "Salt must be stable across loads");

        let decrypted = storage2.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "persisted secret");
    }

    #[test]
    fn salt_file_in_nested_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join("a").join("b").join("c").join(SALT_FILENAME);
        assert!(!salt_path.exists());

        // Should create intermediate directories and succeed.
        let storage = SecureStorage::with_salt_path(&salt_path).unwrap();
        assert!(salt_path.exists());

        let encrypted = storage.encrypt("nested").unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "nested");
    }

    #[test]
    fn corrupt_salt_file_is_regenerated() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);

        // Write a salt file with the wrong length.
        fs::write(&salt_path, b"too-short").unwrap();

        // Should regenerate a valid salt.
        let _storage = SecureStorage::with_salt_path(&salt_path).unwrap();
        let data = fs::read(&salt_path).unwrap();
        assert_eq!(data.len(), SALT_LEN);
    }

    // ---- round-trip with argon2-derived key (integration) ----

    #[test]
    fn roundtrip_with_argon2_derived_key() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);
        let storage = SecureStorage::with_salt_path(&salt_path).unwrap();

        let secrets = [
            "sk-ant-api03-very-secret",
            "",
            "short",
            &"A".repeat(5_000),
        ];
        for secret in &secrets {
            let enc = storage.encrypt(secret).unwrap();
            let dec = storage.decrypt(&enc).unwrap();
            assert_eq!(&dec, secret);
        }
    }

    #[test]
    fn different_salt_files_cannot_cross_decrypt() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        let storage_a = storage_in(tmp_a.path());
        let storage_b = storage_in(tmp_b.path());

        let encrypted = storage_a.encrypt("only for A").unwrap();
        // storage_b has a different random salt, so decryption must fail.
        let result = storage_b.decrypt(&encrypted);
        assert!(result.is_err(), "Different salts must prevent cross-decryption");
    }
}
