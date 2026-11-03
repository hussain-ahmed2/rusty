//! Stable content fingerprints for build artifacts.

/// Returns a stable 64-bit FNV-1a fingerprint as lowercase hexadecimal.
///
/// The fingerprint is for cache keys and change detection, not cryptography.
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
