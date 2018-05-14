//! Transaction prunable data

use chain::TransactionInput;

/// structure for storing prunable transaction data
#[derive(Debug, Clone, Serializable, Deserializable)]
pub struct TransactionPrunableData {
	pub version: i32,
	pub inputs: Vec<TransactionInput>,
	pub total_outputs: u32,
	pub lock_time: u32,
}
