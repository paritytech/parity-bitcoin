use std::collections::HashMap;
use std::collections::hash_map::Entry;
use parking_lot::Mutex;
use network::{ConsensusParams, Deployment};
use hash::H256;
use storage::{BlockHeaderProvider, BlockRef, BlockAncestors, BlockIterator};
use timestamp::median_timestamp;

#[derive(Debug, PartialEq, Clone, Copy)]
enum ThresholdState {
	Defined,
	Started,
	LockedIn,
	Active,
	Failed,
}

impl Default for ThresholdState {
	fn default() -> Self {
		ThresholdState::Defined
	}
}

impl ThresholdState {
	fn is_final(&self) -> bool {
		match *self {
			ThresholdState::Active | ThresholdState::Failed => true,
			ThresholdState::Defined | ThresholdState::Started | ThresholdState::LockedIn => false,
		}
	}

	fn is_active(&self) -> bool {
		match *self {
			ThresholdState::Active => true,
			_ => false,
		}
	}
}

/// Threshold state at given point of time
#[derive(Debug, Clone, Default)]
struct DeploymentState {
	/// Block number
	block_number: u32,
	/// Block hash
	block_hash: H256,
	/// Threshold state for given block
	state: ThresholdState,
}

/// Last known deployment states
type DeploymentStateCache = HashMap<&'static str, DeploymentState>;

#[derive(Default, Debug)]
pub struct Deployments {
	cache: Mutex<DeploymentStateCache>,
}

pub struct BlockDeployments<'a> {
	deployments: &'a Deployments,
	number: u32,
	headers: &'a dyn BlockHeaderProvider,
	consensus: &'a ConsensusParams,
}

impl Deployments {
	pub fn new() -> Self {
		Deployments::default()
	}

	/// Returns true if csv deployment is active
	pub fn csv(&self, number: u32, headers: &dyn BlockHeaderProvider, consensus: &ConsensusParams) -> bool {
		match consensus.csv_deployment {
			Some(csv) => {
				let mut cache = self.cache.lock();
				threshold_state(&mut cache, csv, number, headers, consensus.miner_confirmation_window, consensus.rule_change_activation_threshold).is_active()
			},
			None => false
		}
	}

	/// Returns true if SegWit deployment is active
	pub fn segwit(&self, number: u32, headers: &dyn BlockHeaderProvider, consensus: &ConsensusParams) -> bool {
		match consensus.segwit_deployment {
			Some(segwit) => {
				let mut cache = self.cache.lock();
				threshold_state(&mut cache, segwit, number, headers, consensus.miner_confirmation_window, consensus.rule_change_activation_threshold).is_active()
			},
			None => false
		}
	}
}

impl<'a> BlockDeployments<'a> {
	pub fn new(deployments: &'a Deployments, number: u32, headers: &'a dyn BlockHeaderProvider, consensus: &'a ConsensusParams) -> Self {
		BlockDeployments {
			deployments: deployments,
			number: number,
			headers: headers,
			consensus: consensus,
		}
	}

	pub fn csv(&self) -> bool {
		self.deployments.csv(self.number, self.headers, self.consensus)
	}

	pub fn segwit(&self) -> bool {
		self.deployments.segwit(self.number, self.headers, self.consensus)
	}
}

impl AsRef<Deployments> for Deployments {
	fn as_ref(&self) -> &Deployments {
		&self
	}
}

impl<'a> AsRef<Deployments> for BlockDeployments<'a> {
	fn as_ref(&self) -> &Deployments {
		self.deployments
	}
}

/// Calculates threshold state of given deployment
fn threshold_state(cache: &mut DeploymentStateCache, deployment: Deployment, number: u32, headers: &dyn BlockHeaderProvider, miner_confirmation_window: u32, rule_change_activation_threshold: u32) -> ThresholdState {
	// deployments are checked using previous block index
	if let Some(activation) = deployment.activation {
		if activation <= number {
			return ThresholdState::Active;
		} else {
			return ThresholdState::Defined;
		}
	}

	// number is number of block which is currently validating
	// => it is not in database
	// we need to make all checks for previous blocks
	let number = number.saturating_sub(1);

	// get number of the first block in the period
	let number = first_of_the_period(number, miner_confirmation_window);

	let hash = match headers.block_header(BlockRef::Number(number)) {
		Some(header) => header.hash,
		None => return ThresholdState::Defined,
	};

	match cache.entry(deployment.name) {
		// by checking hash, we make sure we are on the same branch
		Entry::Occupied(ref entry) if entry.get().block_number == number && entry.get().block_hash == hash => {
			entry.get().state
		},
		// otherwise we need to recalculate threshold state
		Entry::Occupied(mut entry) => {
			let deployment_state = entry.get().clone();
			if deployment_state.state.is_final() {
				return deployment_state.state
			}
			let threshold_state = deployment_state.state;
			let deployment_iter = ThresholdIterator::new(deployment, headers, number, miner_confirmation_window, rule_change_activation_threshold, threshold_state);
			let state = deployment_iter.last().expect("iter must have at least one item");
			let result = state.state;
			entry.insert(state);
			result
		},
		Entry::Vacant(entry) => {
			let deployment_iter = ThresholdIterator::new(deployment, headers, miner_confirmation_window - 1, miner_confirmation_window, rule_change_activation_threshold, ThresholdState::Defined);
			let state = deployment_iter.last().unwrap_or_else(|| DeploymentState {
				block_number: number,
				block_hash: hash,
				state: ThresholdState::Defined,
			});
			let result = state.state;
			entry.insert(state);
			result
		},
	}
}

fn first_of_the_period(block: u32, miner_confirmation_window: u32) -> u32 {
	if block < miner_confirmation_window - 1 {
		0
	} else {
		block - ((block + 1) % miner_confirmation_window)
	}
}

fn count_deployment_matches(block_number: u32, blocks: &dyn BlockHeaderProvider, deployment: Deployment, window: u32) -> usize {
	BlockAncestors::new(BlockRef::Number(block_number), blocks)
		.take(window as usize)
		.filter(|header| deployment.matches(header.raw.version))
		.count()
}

struct ThresholdIterator<'a> {
	deployment: Deployment,
	block_iterator: BlockIterator<'a>,
	headers: &'a dyn BlockHeaderProvider,
	miner_confirmation_window: u32,
	rule_change_activation_threshold: u32,
	last_state: ThresholdState,
}

impl<'a> ThresholdIterator<'a> {
	fn new(deployment: Deployment, headers: &'a dyn BlockHeaderProvider, to_check: u32, miner_confirmation_window: u32, rule_change_activation_threshold: u32, state: ThresholdState) -> Self {
		ThresholdIterator {
			deployment: deployment,
			block_iterator: BlockIterator::new(to_check, miner_confirmation_window, headers),
			headers: headers,
			miner_confirmation_window: miner_confirmation_window,
			rule_change_activation_threshold: rule_change_activation_threshold,
			last_state: state,
		}
	}
}

impl<'a> Iterator for ThresholdIterator<'a> {
	type Item = DeploymentState;

	fn next(&mut self) -> Option<Self::Item> {
		let (block_number, header) = match self.block_iterator.next() {
			Some(header) => header,
			None => return None,
		};

		let median = median_timestamp(&header.raw, self.headers);

		match self.last_state {
			ThresholdState::Defined => {
				if median >= self.deployment.timeout {
					self.last_state = ThresholdState::Failed;
				} else if median >= self.deployment.start_time {
					self.last_state = ThresholdState::Started;
				}
			},
			ThresholdState::Started => {
				if median >= self.deployment.timeout {
					self.last_state = ThresholdState::Failed;
				} else {
					let count = count_deployment_matches(block_number, self.headers, self.deployment, self.miner_confirmation_window);
					if count >= self.rule_change_activation_threshold as usize {
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

		let result = DeploymentState {
			block_number: block_number,
			block_hash: header.hash,
			state: self.last_state,
		};

		Some(result)
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::collections::HashMap;
	use chain::{BlockHeader, IndexedBlockHeader};
	use storage::{BlockHeaderProvider, BlockRef};
	use network::Deployment;
	use hash::H256;
	use primitives::bytes::Bytes;
	use super::{DeploymentStateCache, ThresholdState, first_of_the_period, threshold_state};

	const MINER_CONFIRMATION_WINDOW: u32 = 1000;
	const RULE_CHANGE_ACTIVATION_THRESHOLD: u32 = 900;

	#[derive(Default)]
	struct DeploymentHeaderProvider {
		pub request_count: AtomicUsize,
		pub by_height: Vec<BlockHeader>,
		pub by_hash: HashMap<H256, usize>,
	}

	impl DeploymentHeaderProvider {
		pub fn mine(&mut self, height: u32, time: u32, version: u32) {
			let mut previous_header_hash = self.by_height.last().map(|h| h.previous_header_hash.clone()).unwrap_or(H256::default());
			while self.by_height.len() < height as usize {
				let header = BlockHeader {
					version: version,
					previous_header_hash: previous_header_hash,
					merkle_root_hash: Default::default(),
					time: time,
					bits: 0.into(),
					nonce: height,
				};
				previous_header_hash = header.hash();

				self.by_height.push(header);
				self.by_hash.insert(previous_header_hash.clone(), self.by_height.len() - 1);
			}
		}
	}

	impl BlockHeaderProvider for DeploymentHeaderProvider {
		fn block_header_bytes(&self, _block_ref: BlockRef) -> Option<Bytes> {
			unimplemented!()
		}

		fn block_header(&self, block_ref: BlockRef) -> Option<IndexedBlockHeader> {
			self.request_count.fetch_add(1, Ordering::Relaxed);
			match block_ref {
				BlockRef::Number(height) => self.by_height.get(height as usize).cloned(),
				BlockRef::Hash(hash) => self.by_hash.get(&hash).and_then(|height| self.by_height.get(*height)).cloned(),
			}.map(Into::into)
		}
	}

	fn make_test_time(height: u32) -> u32 {
		1415926536 + 600 * height
	}

	fn prepare_deployments() -> (DeploymentStateCache, DeploymentHeaderProvider, Deployment) {
		let (mut cache, headers, deployment) = (
			DeploymentStateCache::default(),
			DeploymentHeaderProvider::default(),
			Deployment {
				name: "test",
				bit: 0,
				start_time: make_test_time(10000),
				timeout: make_test_time(20000),
				activation: None,
			},
		);

		assert_eq!(threshold_state(&mut cache, deployment, 0, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), ThresholdState::Defined);
		assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, 0, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), ThresholdState::Defined);

		(cache, headers, deployment)
	}

	#[test]
	fn test_first_of_the_period() {
		let window = 2016;
		assert_eq!(0, first_of_the_period(0, window));
		assert_eq!(0, first_of_the_period(1, window));
		assert_eq!(0, first_of_the_period(2014, window));
		assert_eq!(2015, first_of_the_period(2015, window));
		assert_eq!(2015, first_of_the_period(2016, window));
		assert_eq!(8063, first_of_the_period(8063, window));
		assert_eq!(8063, first_of_the_period(10000, window));
		assert_eq!(8063, first_of_the_period(10001, window));
	}

	// https://github.com/bitcoin/bitcoin/blob/a90e6d2bffc422ddcdb771c53aac0bceb970a2c4/src/test/versionbits_tests.cpp#L149
	#[test]
	fn test_threshold_state_defined_to_failed() {
		let (mut cache, mut headers, deployment) = prepare_deployments();
		let test_cases = vec![
			(1,		make_test_time(1),		0x20000001,	ThresholdState::Defined),
			(11,	make_test_time(11),		0x20000001,	ThresholdState::Defined),
			(989,	make_test_time(989),	0x20000001,	ThresholdState::Defined),
			(999,	make_test_time(20000),	0x20000001,	ThresholdState::Defined),
			(1000,	make_test_time(30000),	0x20000001,	ThresholdState::Failed),
			(1999,	make_test_time(30001),	0x20000001,	ThresholdState::Failed),
			(2000,	make_test_time(30002),	0x20000001,	ThresholdState::Failed),
			(2001,	make_test_time(30003),	0x20000001,	ThresholdState::Failed),
			(2999,	make_test_time(30004),	0x20000001,	ThresholdState::Failed),
			(3000,	make_test_time(30005),	0x20000001,	ThresholdState::Failed),
		];

		for (height, time, version, state) in test_cases {
			headers.mine(height, time, version);

			assert_eq!(threshold_state(&mut cache, deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
			assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
		}
	}

	// https://github.com/bitcoin/bitcoin/blob/a90e6d2bffc422ddcdb771c53aac0bceb970a2c4/src/test/versionbits_tests.cpp#L162
	#[test]
	fn test_threshold_state_defined_to_started_to_failed() {
		let (mut cache, mut headers, deployment) = prepare_deployments();
		let test_cases = vec![
			(1,		make_test_time(1),			0x20000000,	ThresholdState::Defined),
			(1000,	make_test_time(10000) - 1,	0x20000001,	ThresholdState::Defined),
			(2000,	make_test_time(10000),		0x20000001,	ThresholdState::Started),
			(2051,	make_test_time(10010),		0x20000000,	ThresholdState::Started),
			(2950,	make_test_time(10020),		0x20000001,	ThresholdState::Started),
			(3000,	make_test_time(20000),		0x20000000,	ThresholdState::Failed),
			(4000,	make_test_time(20010),		0x20000001,	ThresholdState::Failed),
		];


		for (height, time, version, state) in test_cases {
			headers.mine(height, time, version);

			assert_eq!(threshold_state(&mut cache, deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
			assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
		}
	}

	// https://github.com/bitcoin/bitcoin/blob/a90e6d2bffc422ddcdb771c53aac0bceb970a2c4/src/test/versionbits_tests.cpp#L172
	#[test]
	fn test_threshold_state_defined_to_started_to_failed_when_threshold_reached() {
		let (mut cache, mut headers, deployment) = prepare_deployments();
		let test_cases = vec![
			(1,		make_test_time(1),			0x20000000,	ThresholdState::Defined),
			(1000,	make_test_time(10000) - 1,	0x20000001,	ThresholdState::Defined),
			(2000,	make_test_time(10000),		0x20000001,	ThresholdState::Started),
			(2999,	make_test_time(30000),		0x20000001,	ThresholdState::Started),
			(3000,	make_test_time(30000),		0x20000001,	ThresholdState::Failed),
			(3999,	make_test_time(30001),		0x20000000,	ThresholdState::Failed),
			(4000,	make_test_time(30002),		0x20000000,	ThresholdState::Failed),
			(14333,	make_test_time(30003),		0x20000000,	ThresholdState::Failed),
			(20000,	make_test_time(40000),		0x20000000,	ThresholdState::Failed),
		];

		for (height, time, version, state) in test_cases {
			headers.mine(height, time, version);

			assert_eq!(threshold_state(&mut cache, deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
			assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
		}
	}

	// https://github.com/bitcoin/bitcoin/blob/a90e6d2bffc422ddcdb771c53aac0bceb970a2c4/src/test/versionbits_tests.cpp#L184
	#[test]
	fn test_threshold_state_defined_to_started_to_lockedin() {
		let (mut cache, mut headers, deployment) = prepare_deployments();
		let test_cases = vec![
			(1,		make_test_time(1),			0x20000000,	ThresholdState::Defined, false),
			(1000,	make_test_time(10000) - 1,	0x20000001,	ThresholdState::Defined, false),
			(2000,	make_test_time(10000),		0x20000001,	ThresholdState::Started, false),
			(2050,	make_test_time(10010),		0x20000000,	ThresholdState::Started, true),
			(2950,	make_test_time(10020),		0x20000001,	ThresholdState::Started, true),
			(2999,	make_test_time(19999),		0x20000000,	ThresholdState::Started, true),
			(3000,	make_test_time(29999),		0x20000000,	ThresholdState::LockedIn, false),
			(3999,	make_test_time(30001),		0x20000000,	ThresholdState::LockedIn, true),
			(4000,	make_test_time(30002),		0x20000000,	ThresholdState::Active, false),
			(14333,	make_test_time(30003),		0x20000000,	ThresholdState::Active, false),
			(24000,	make_test_time(40000),		0x20000000,	ThresholdState::Active, false),
		];

		for (height, time, version, state, is_single_request) in test_cases {
			headers.mine(height, time, version);

			let req_old = headers.request_count.load(Ordering::Relaxed);
			assert_eq!(threshold_state(&mut cache, deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
			let req_new = headers.request_count.load(Ordering::Relaxed);

			// also check that same-period states are read from cache
			if is_single_request {
				assert_eq!(req_old + 1, req_new);
			} else {
				assert!(req_old < req_new);
			}

			assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
		}
	}

	// https://github.com/bitcoin/bitcoin/blob/a90e6d2bffc422ddcdb771c53aac0bceb970a2c4/src/test/versionbits_tests.cpp#L198
	#[test]
	fn test_threshold_state_defined_multiple_to_started_multiple_to_failed() {
		let (mut cache, mut headers, deployment) = prepare_deployments();
		let test_cases = vec![
			(999,	make_test_time(999),		0x20000000,	ThresholdState::Defined),
			(1000,	make_test_time(1000),		0x20000000,	ThresholdState::Defined),
			(2000,	make_test_time(2000),		0x20000000,	ThresholdState::Defined),
			(3000,	make_test_time(10000),		0x20000000,	ThresholdState::Started),
			(4000,	make_test_time(10000),		0x20000000,	ThresholdState::Started),
			(5000,	make_test_time(10000),		0x20000000,	ThresholdState::Started),
			(6000,	make_test_time(20000),		0x20000000,	ThresholdState::Failed),
			(7000,	make_test_time(20000),		0x20000001,	ThresholdState::Failed),
		];

		for (height, time, version, state) in test_cases {
			headers.mine(height, time, version);

			assert_eq!(threshold_state(&mut cache, deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
			assert_eq!(threshold_state(&mut DeploymentStateCache::default(), deployment, height, &headers, MINER_CONFIRMATION_WINDOW, RULE_CHANGE_ACTIVATION_THRESHOLD), state);
		}
	}
}
