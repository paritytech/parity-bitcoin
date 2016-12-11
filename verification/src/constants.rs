//! Consenus constants

pub const BLOCK_MAX_FUTURE: i64 = 2 * 60 * 60; // 2 hours
pub const COINBASE_MATURITY: u32 = 100; // 2 hours
pub const MAX_BLOCK_SIZE: usize = 1_000_000;
pub const MAX_BLOCK_SIGOPS: usize = 20_000;
pub const MIN_COINBASE_SIZE: usize = 2;
pub const MAX_COINBASE_SIZE: usize = 100;
