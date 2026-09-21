use sha2::Sha256;
use sha3::{Digest, Sha3_256};
use std::fmt::Write as _;

pub(crate) fn sha3_256(bytes: &[u8]) -> String {
    encode(&Sha3_256::digest(bytes))
}

pub(crate) fn sha3_256_parts(parts: &[&[u8]]) -> String {
    let mut hasher = Sha3_256::new();
    for part in parts {
        hasher.update(part);
        hasher.update([0]);
    }
    encode(&hasher.finalize())
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    let mut out = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn encode(hash: &[u8]) -> String {
    let mut out = String::from("sha3-256:");
    for byte in hash {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
