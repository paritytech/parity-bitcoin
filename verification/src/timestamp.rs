use chain::BlockHeader;
use storage::{BlockHeaderProvider, BlockAncestors};
use primitives::hash::H256;

/// Returns median timestamp, of given header ancestors.
/// The header should be later expected to have higher timestamp
/// than this median timestamp
pub fn median_timestamp(header: &BlockHeader, store: &dyn BlockHeaderProvider) -> u32 {
	median_timestamp_inclusive(header.previous_header_hash.clone(), store)
}

/// Returns median timestamp, of given header + its ancestors.
/// The header should be later expected to have higher timestamp
/// than this median timestamp
pub fn median_timestamp_inclusive(previous_header_hash: H256, store: &dyn BlockHeaderProvider) -> u32 {
	let mut timestamps: Vec<_> = BlockAncestors::new(previous_header_hash.clone().into(), store)
		.take(11)
		.map(|header| header.raw.time)
		.collect();

	if timestamps.is_empty() {
		return 0;
	}

	timestamps.sort();
	timestamps[timestamps.len() / 2]
}
