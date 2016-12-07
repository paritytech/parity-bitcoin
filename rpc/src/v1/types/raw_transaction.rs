//use serde::{Serialize, Deserialize};
use super::bytes::Bytes;

/// Raw (serialized) transaction
#[derive(Debug, Serialize, Deserialize)]
pub struct RawTransaction {
	/// Serialized transaction data bytes
	pub data: Bytes,
}
