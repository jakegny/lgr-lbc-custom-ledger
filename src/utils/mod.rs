use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::{fs, io};

pub mod hashing;

// Re-export the double_sha256 function
pub use hashing::*;

pub fn save_key_pair(
    secret_key: &SecretKey,
    public_key: &PublicKey,
    filepath: &str,
) -> io::Result<()> {
    let sk_hex = hex::encode(secret_key.secret_bytes());
    let pk_hex = hex::encode(public_key.serialize());
    let keypair = format!("{}\n{}", sk_hex, pk_hex);
    fs::write(filepath, keypair)
}

pub fn load_key_pair(filepath: &str) -> io::Result<(SecretKey, PublicKey)> {
    let secp = Secp256k1::new();
    let content = fs::read_to_string(filepath)?;
    let mut lines = content.lines();
    let sk_hex = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing secret key"))?;
    let pk_hex = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing public key"))?;
    let sk_bytes = hex::decode(sk_hex)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid secret key hex"))?;
    let pk_bytes = hex::decode(pk_hex)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid public key hex"))?;
    let secret_key = SecretKey::from_slice(&sk_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid secret key bytes"))?;
    let public_key = PublicKey::from_slice(&pk_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid public key bytes"))?;
    Ok((secret_key, public_key))
}

pub const DIFFICULTY: u32 = 0x1e0fffff; // 0x1f00ffff; // 0x1d00ffff; // Bitcoin's initial difficulty target
                                        // medium difficulty: 0x1d10ffff
                                        // low difficulty: 0x1e00ffff
                                        // 0x1e0fffff betweenlow and medium (in theory)
