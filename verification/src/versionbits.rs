use std::collections::HashMap;
use network::Magic;
use hash::H256;
use chain::BlockHeader;
use db::{BlockHeaderProvider, BlockProvider, BlockRef, BlockAncestors};
use timestamp::median_timestamp;

const VERSIONBITS_TOP_MASK: u32 = 0xe0000000;
const VERSIONBITS_TOP_BITS: u32 = 0x20000000;

#[derive(Debug, Clone, Copy)]
pub enum ThresholdState {
	Defined,
	Started,
	LockedIn,
	Active,
	Failed,
}

pub struct ThresholdConditionCache {
	map: HashMap<&'static str, HashMap<u32, ThresholdState>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Condition {
	pub name: &'static str,
	pub bit: u8,
	pub start_time: u32,
	pub timeout: u32,
}

impl Condition {
	pub fn csv(network: Magic) -> Option<Self> {
		let (start_time, timeout) = match network {
			Magic::Mainnet => (1462060800, 1493596800),
			Magic::Testnet => (1456790400, 1493596800),
			_ => { return None }
		};

		let condition = Condition {
			name: "csv",
			bit: 0,
			start_time: start_time,
			timeout: timeout,
		};

		Some(condition)
	}

	pub fn segwit(network: Magic) -> Option<Self> {
		let (start_time, timeout) = match network {
			Magic::Mainnet => (1479168000, 1510704000),
			Magic::Testnet => (1462060800, 1493596800),
			_ => { return None },
		};

		let condition = Condition {
			name: "segwit",
			bit: 1,
			start_time: start_time,
			timeout: timeout,
		};

		Some(condition)
	}

	pub fn matches(&self, version: u32) -> bool {
		(version & VERSIONBITS_TOP_MASK) == VERSIONBITS_TOP_BITS && (version & (1 << self.bit)) != 0
	}
}

pub fn threshold_state(cache: &mut ThresholdConditionCache, condition: Condition, block: &H256, headers: &BlockProvider) -> ThresholdState {
	// A block's state is always the same as that of the first of its period, so it is computed based on a
	// pindexPrev whose height equals a multiple of nPeriod - 1.

	// get number of the first block in the period
	let number = 0;

	{
		let condition_cache = cache.map.get(condition.name).expect("condition cache expected to be known");
		if let Some(state) = condition_cache.get(&number) {
			return *state;
		}
	}

	unimplemented!();
}

fn first_of_the_period(block: u32) -> u32 {
	block - ((block + 1) % 2016)
}

fn count_condition_matches(to_check: u32, blocks: &BlockHeaderProvider, condition: Condition) -> usize {
	BlockAncestors::new(BlockRef::Number(to_check), blocks)
		.take(2016)
		.filter(|header| condition.matches(header.version))
		.count()
}

struct ThresholdConditionIterator<'a> {
	condition: Condition,
	blocks: &'a BlockHeaderProvider,
	to_check: u32,
	network: Magic,
	last_state: ThresholdState,
}

impl<'a> ThresholdConditionIterator<'a> {
	pub fn new(condition: Condition, blocks: &'a BlockHeaderProvider, to_check: u32, network: Magic) -> Self {
		ThresholdConditionIterator {
			condition: condition,
			blocks: blocks,
			to_check: first_of_the_period(to_check),
			network: network,
			last_state: ThresholdState::Defined,
		}
	}
}

impl<'a> Iterator for ThresholdConditionIterator<'a> {
	type Item = ThresholdState;

	fn next(&mut self) -> Option<Self::Item> {
		let header = match self.blocks.block_header(BlockRef::Number(self.to_check)) {
			Some(header) => header,
			None => return None,
		};

		let median = median_timestamp(&header, self.blocks, self.network);

		match self.last_state {
			ThresholdState::Defined => {
				if median >= self.condition.timeout {
					self.last_state = ThresholdState::Failed;
				} else if median >= self.condition.start_time {
					self.last_state = ThresholdState::Started;
				}
			},
			ThresholdState::Started => {
				if median >= self.condition.timeout {
					self.last_state = ThresholdState::Failed;
				} else {
					let count = count_condition_matches(self.to_check, self.blocks, self.condition);
					// TODO: call if threshold and move it to consensus params
					if count >= 1916 {
						self.last_state = ThresholdState::LockedIn;
					}
				}
			},
			ThresholdState::LockedIn => {
				self.last_state = ThresholdState::Active;
			},
			ThresholdState::Failed | ThresholdState::Active => {
				return None
			}
		}

		self.to_check += 2016;
		Some(self.last_state)
	}
}
