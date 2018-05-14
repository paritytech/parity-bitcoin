use chain::TransactionOutput;

/// structure for indexing transaction output info
#[derive(Debug, Clone, Serializable, Deserializable)]
pub struct TransactionOutputMeta {
	/// transaction output
	pub output: TransactionOutput,
	/// is this output spent?
	pub is_spent: bool,
}

impl TransactionOutputMeta {
	pub fn new(output: TransactionOutput) -> Self {
		TransactionOutputMeta {
			output,
			is_spent: false,
		}
	}
}
