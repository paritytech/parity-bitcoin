use std::collections::BTreeSet;
use chain::BlockHeader;
use db::BlockHeaderProvider;
use network::Magic;

pub fn median_timestamp(header: &BlockHeader, store: &BlockHeaderProvider, network: Magic) -> u32 {
	// TODO: timestamp validation on testnet is broken
	if network == Magic::Testnet {
		return 0;
	}

	let ancestors = 11;
	let mut timestamps = BTreeSet::new();
	let mut block_ref = header.previous_header_hash.clone().into();

	for _ in 0..ancestors {
		let previous_header = match store.block_header(block_ref) {
			Some(h) => h,
			None => break,
		};
		timestamps.insert(previous_header.time);
		block_ref = previous_header.previous_header_hash.into();
	}

	if timestamps.is_empty() {
		return 0;
	}

	let timestamps = timestamps.into_iter().collect::<Vec<_>>();
	timestamps[timestamps.len() / 2]
}
