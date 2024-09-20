use sha2::{Digest, Sha256};

/// Computes a double SHA-256 hash of the input data.
///
/// # Arguments
///
/// * `data` - A byte slice representing the data to hash.
///
/// # Returns
///
/// A `Vec<u8>` containing the double SHA-256 hash of the input data.
pub fn double_sha256(data: &[u8]) -> Vec<u8> {
    let hash1 = Sha256::digest(data);
    let hash2 = Sha256::digest(&hash1);
    hash2.to_vec()
}
