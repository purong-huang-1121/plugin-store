//! Polymarket CLOB authentication.
//!
//! Two auth levels:
//! - L1 (EIP-712): Used to derive API credentials from a private key.
//! - L2 (HMAC-SHA256): Used for all authenticated API requests.

use anyhow::{Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use k256::ecdsa::{SigningKey, VerifyingKey};
use sha2::Sha256;
use tiny_keccak::{Hasher, Keccak};

const CLOB_DOMAIN_NAME: &str = "ClobAuthDomain";
const CLOB_VERSION: &str = "1";
const MSG_TO_SIGN: &str = "This message attests that I control the given wallet";

/// Polymarket API credentials.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiCreds {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

/// Build HMAC-SHA256 signature for L2 authenticated requests.
pub fn build_hmac_signature(
    secret: &str,
    timestamp: &str,
    method: &str,
    request_path: &str,
    body: Option<&str>,
) -> Result<String> {
    let secret_bytes = base64::engine::general_purpose::URL_SAFE
        .decode(secret)
        .context("failed to decode API secret")?;

    let mut message = format!("{}{}{}", timestamp, method, request_path);
    if let Some(b) = body {
        message.push_str(b);
    }

    let mut mac =
        Hmac::<Sha256>::new_from_slice(&secret_bytes).expect("HMAC accepts any key length");
    mac.update(message.as_bytes());

    Ok(base64::engine::general_purpose::URL_SAFE.encode(mac.finalize().into_bytes()))
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// Compute EIP-712 domain separator.
fn domain_separator(chain_id: u64) -> [u8; 32] {
    let type_hash = keccak256(b"EIP712Domain(string name,string version,uint256 chainId)");
    let name_hash = keccak256(CLOB_DOMAIN_NAME.as_bytes());
    let version_hash = keccak256(CLOB_VERSION.as_bytes());

    let mut encoded = Vec::with_capacity(128);
    encoded.extend_from_slice(&type_hash);
    encoded.extend_from_slice(&name_hash);
    encoded.extend_from_slice(&version_hash);
    let mut chain_id_bytes = [0u8; 32];
    chain_id_bytes[24..].copy_from_slice(&chain_id.to_be_bytes());
    encoded.extend_from_slice(&chain_id_bytes);

    keccak256(&encoded)
}

/// Compute EIP-712 struct hash for ClobAuth.
fn clob_auth_struct_hash(address: &str, timestamp: &str, nonce: u64) -> [u8; 32] {
    let type_hash =
        keccak256(b"ClobAuth(address address,string timestamp,uint256 nonce,string message)");

    let mut addr_bytes = [0u8; 32];
    let addr_raw =
        hex::decode(address.strip_prefix("0x").unwrap_or(address)).expect("invalid address hex");
    addr_bytes[12..].copy_from_slice(&addr_raw);

    let timestamp_hash = keccak256(timestamp.as_bytes());
    let message_hash = keccak256(MSG_TO_SIGN.as_bytes());

    let mut nonce_bytes = [0u8; 32];
    nonce_bytes[24..].copy_from_slice(&nonce.to_be_bytes());

    let mut encoded = Vec::with_capacity(160);
    encoded.extend_from_slice(&type_hash);
    encoded.extend_from_slice(&addr_bytes);
    encoded.extend_from_slice(&timestamp_hash);
    encoded.extend_from_slice(&nonce_bytes);
    encoded.extend_from_slice(&message_hash);

    keccak256(&encoded)
}

/// Sign the ClobAuth EIP-712 message.
pub fn sign_clob_auth(
    signing_key: &SigningKey,
    address: &str,
    timestamp: &str,
    nonce: u64,
    chain_id: u64,
) -> Result<String> {
    let domain_sep = domain_separator(chain_id);
    let struct_hash = clob_auth_struct_hash(address, timestamp, nonce);

    let mut msg = Vec::with_capacity(66);
    msg.push(0x19);
    msg.push(0x01);
    msg.extend_from_slice(&domain_sep);
    msg.extend_from_slice(&struct_hash);
    let digest = keccak256(&msg);

    let (signature, recovery_id) = signing_key
        .sign_prehash_recoverable(&digest)
        .context("signing failed")?;

    let mut sig_bytes = [0u8; 65];
    sig_bytes[..64].copy_from_slice(&signature.to_bytes());
    sig_bytes[64] = recovery_id.to_byte() + 27;

    Ok(format!("0x{}", hex::encode(sig_bytes)))
}

/// Derive Ethereum address from signing key (EIP-55 checksummed).
pub fn address_from_key(signing_key: &SigningKey) -> String {
    let verifying_key = VerifyingKey::from(signing_key);
    let pubkey_bytes = verifying_key.to_encoded_point(false);
    let hash = keccak256(&pubkey_bytes.as_bytes()[1..]);
    let addr_hex = hex::encode(&hash[12..]);
    eip55_checksum(&addr_hex)
}

/// EIP-55 mixed-case checksum encoding.
fn eip55_checksum(address: &str) -> String {
    let addr_lower = address.to_lowercase();
    let hash = keccak256(addr_lower.as_bytes());
    let hash_hex = hex::encode(hash);

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");
    for (i, c) in addr_lower.chars().enumerate() {
        if c.is_ascii_alphabetic() {
            let nibble = u8::from_str_radix(&hash_hex[i..i + 1], 16).unwrap();
            if nibble >= 8 {
                checksummed.push(c.to_ascii_uppercase());
            } else {
                checksummed.push(c);
            }
        } else {
            checksummed.push(c);
        }
    }
    checksummed
}

/// Load signing key from EVM_PRIVATE_KEY env var.
pub fn load_signing_key() -> Result<SigningKey> {
    let pk_hex = std::env::var("EVM_PRIVATE_KEY")
        .context("EVM_PRIVATE_KEY not set — required for trading commands")?;
    let pk_hex = pk_hex.strip_prefix("0x").unwrap_or(&pk_hex);
    let pk_bytes = hex::decode(pk_hex).context("invalid private key hex")?;
    SigningKey::from_bytes(pk_bytes.as_slice().into()).context("invalid private key")
}

/// Load API credentials from env vars or cache.
pub fn load_api_creds() -> Result<Option<ApiCreds>> {
    if let (Ok(key), Ok(secret), Ok(pass)) = (
        std::env::var("POLYMARKET_API_KEY"),
        std::env::var("POLYMARKET_SECRET"),
        std::env::var("POLYMARKET_PASSPHRASE"),
    ) {
        return Ok(Some(ApiCreds {
            api_key: key,
            secret,
            passphrase: pass,
        }));
    }

    let cache_path = dirs::home_dir().map(|h| h.join(".plugin-store").join("polymarket_creds.json"));

    if let Some(ref path) = cache_path {
        if path.exists() {
            let data = std::fs::read_to_string(path).ok();
            if let Some(data) = data {
                if let Ok(creds) = serde_json::from_str::<ApiCreds>(&data) {
                    return Ok(Some(creds));
                }
            }
        }
    }

    Ok(None)
}

/// Save API credentials to cache.
pub fn save_api_creds(creds: &ApiCreds) -> Result<()> {
    let cache_dir = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".plugin-store");
    std::fs::create_dir_all(&cache_dir)?;
    let path = cache_dir.join("polymarket_creds.json");
    let data = serde_json::to_string_pretty(creds)?;
    std::fs::write(&path, &data)?;
    Ok(())
}
