use sha3::{Digest, Keccak256};
use std::fmt::Write;

/// Ethereum Keccak-256, not FIPS SHA3-256.
#[must_use]
pub(super) fn keccak256(bytes: &[u8]) -> [u8; 32] {
    Keccak256::digest(bytes).into()
}

#[must_use]
pub(super) fn selector(signature: &str) -> String {
    let digest = keccak256(signature.as_bytes());
    format!(
        "0x{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3]
    )
}

#[must_use]
pub(super) fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::from("0x");
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{hex32, keccak256, selector};

    #[test]
    fn known_transfer_selector() {
        assert_eq!(selector("transfer(address,uint256)"), "0xa9059cbb");
    }

    #[test]
    fn digest_is_32_bytes() {
        assert_eq!(hex32(&keccak256(b"")).len(), 66);
    }
}
