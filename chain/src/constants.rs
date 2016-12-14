
// Below flags apply in the context of BIP 68
// If this flag set, CTxIn::nSequence is NOT interpreted as a
// relative lock-time.
pub const SEQUENCE_LOCKTIME_DISABLE_FLAG: u32 = 1u32 << 31;

// Setting nSequence to this value for every input in a transaction
// disables nLockTime.
pub const SEQUENCE_FINAL: u32 = 0xffffffff;

// If CTxIn::nSequence encodes a relative lock-time and this flag
// is set, the relative lock-time has units of 512 seconds,
// otherwise it specifies blocks with a granularity of 1.
pub const SEQUENCE_LOCKTIME_TYPE_FLAG: u32 = (1 << 22);

// If CTxIn::nSequence encodes a relative lock-time, this mask is
// applied to extract that lock-time from the sequence field.
pub const SEQUENCE_LOCKTIME_MASK: u32 = 0x0000ffff;

/// Threshold for `nLockTime`: below this value it is interpreted as block number,
/// otherwise as UNIX timestamp.
pub const LOCKTIME_THRESHOLD: u32 = 500000000; // Tue Nov  5 00:53:20 1985 UTC

/// Number of Satoshis in single coin
pub const SATOSHIS_IN_COIN: u64 = 100_000_000;
