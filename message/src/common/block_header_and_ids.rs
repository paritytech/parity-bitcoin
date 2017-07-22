use chain::{BlockHeader, ShortTransactionID};
use common::PrefilledTransaction;

#[derive(Debug, PartialEq, Serializable, Deserializable)]
pub struct BlockHeaderAndIDs {
	pub header: BlockHeader,
	pub nonce: u64,
	pub short_ids: Vec<ShortTransactionID>,
	pub prefilled_transactions: Vec<PrefilledTransaction>,
}
