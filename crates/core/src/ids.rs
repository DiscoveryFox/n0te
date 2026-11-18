//! Identifiers: random-looking, collision-free across devices without coordination.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 64 random-ish bits from the clock, a process-wide counter, the process id and `salt`.
pub fn random_u64(salt: u64) -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut h = blake3::Hasher::new();
    h.update(&now.to_le_bytes());
    h.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    h.update(&std::process::id().to_le_bytes());
    h.update(&salt.to_le_bytes());
    let b = h.finalize();
    u64::from_le_bytes(b.as_bytes()[..8].try_into().expect("8 bytes"))
}

/// A new id as 16 hex digits.
pub fn new_id(salt: u64) -> String {
    format!("{:016x}", random_u64(salt))
}

/// A Loro peer id for this device. Must differ between devices editing the same document;
/// the store keeps one per library.
pub fn new_peer_id() -> u64 {
    // Loro reserves the top values; keep it in the safe range.
    random_u64(0x6e30_7465) >> 1
}
