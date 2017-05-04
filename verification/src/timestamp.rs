use std::collections::BTreeSet;
use chain::BlockHeader;
use db::{BlockHeaderProvider, BlockAncestors};

/// Returns median timestamp, of given header ancestors.
/// The header should be later expected to have higher timestamp
/// than this median timestamp
pub fn median_timestamp(header: &BlockHeader, store: &BlockHeaderProvider) -> u32 {
	let timestamps: BTreeSet<_> = BlockAncestors::new(header.previous_header_hash.clone().into(), store)
		.take(11)
		.map(|header| header.time)
		.collect();

	if timestamps.is_empty() {
		return 0;
	}

	let timestamps = timestamps.into_iter().collect::<Vec<_>>();
	timestamps[timestamps.len() / 2]
}
