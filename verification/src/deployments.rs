use std::collections::HashMap;
use std::collections::hash_map::Entry;
use parking_lot::Mutex;
use network::{ConsensusParams, Deployment, Deployments as NetworkDeployments};
use hash::H256;
use db::{BlockHeaderProvider, BlockRef, BlockAncestors, BlockIterator};
use timestamp::median_timestamp;

#[derive(Debug, Clone, Copy)]
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

#[derive(Clone, Copy)]
pub struct ActiveDeployments<'a> {
	pub deployments: &'a Deployments,
	number: u32,
	headers: &'a BlockHeaderProvider,
	consensus: &'a ConsensusParams,
}

impl Deployments {
	pub fn new() -> Self {
		Deployments::default()
	}

	/// Returns true if csv deployment is active
	pub fn csv(&self, number: u32, headers: &BlockHeaderProvider, consensus: &ConsensusParams) -> bool {
		match consensus.csv_deployment {
			Some(csv) => {
				let mut cache = self.cache.lock();
				threshold_state(&mut cache, csv, number, headers, consensus).is_active()
			},
			None => false
		}
	}

	/// Returns true if SegWit deployment is active
	pub fn segwit(&self, number: u32, headers: &BlockHeaderProvider, consensus: &ConsensusParams) -> bool {
		match consensus.segwit_deployment {
			Some(segwit) => {
				let mut cache = self.cache.lock();
				threshold_state(&mut cache, segwit, number, headers, consensus).is_active()
			},
			None => false
		}
	}
}

/// Calculates threshold state of given deployment
fn threshold_state(cache: &mut DeploymentStateCache, deployment: Deployment, number: u32, headers: &BlockHeaderProvider, consensus: &ConsensusParams) -> ThresholdState {
	if let Some(activation) = deployment.activation {
		if activation <= number {
			return ThresholdState::Active;
		} else {
			return ThresholdState::Defined;
		}
	}

	// get number of the first block in the period
	let number = first_of_the_period(number, consensus.miner_confirmation_window);

	let hash = match headers.block_header(BlockRef::Number(number)) {
		Some(header) => header.hash(),
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
			let from_block = deployment_state.block_number + consensus.miner_confirmation_window;
			let threshold_state = deployment_state.state;
			let deployment_iter = ThresholdIterator::new(deployment, headers, from_block, consensus, threshold_state);
			let state = deployment_iter.last().expect("iter must have at least one item");
			let result = state.state;
			entry.insert(state);
			result
		},
		Entry::Vacant(entry) => {
			let deployment_iter = ThresholdIterator::new(deployment, headers, 0, consensus, ThresholdState::Defined);
			let state = deployment_iter.last().unwrap_or_default();
			let result = state.state;
			entry.insert(state);
			result
		},
	}

}

impl<'a> ActiveDeployments<'a> {
	pub fn new(deployments: &'a Deployments, number: u32, headers: &'a BlockHeaderProvider, consensus: &'a ConsensusParams) -> Self {
		ActiveDeployments {
			deployments: deployments,
			number: number,
			headers: headers,
			consensus: consensus,
		}
	}
}

impl<'a> NetworkDeployments for ActiveDeployments<'a> {
	fn is_active(&self, name: &str) -> bool {
		match name {
			"csv" => self.deployments.segwit(self.number, self.headers, self.consensus),
			"segwit" => self.deployments.segwit(self.number, self.headers, self.consensus),
			_ => false,
		}
	}
}

fn first_of_the_period(block: u32, miner_confirmation_window: u32) -> u32 {
	if block < miner_confirmation_window - 1 {
		0
	} else {
		block - ((block + 1) % miner_confirmation_window)
	}
}

fn count_deployment_matches(block_number: u32, blocks: &BlockHeaderProvider, deployment: Deployment, window: u32) -> usize {
	BlockAncestors::new(BlockRef::Number(block_number), blocks)
		.take(window as usize)
		.filter(|header| deployment.matches(header.version))
		.count()
}

struct ThresholdIterator<'a> {
	deployment: Deployment,
	block_iterator: BlockIterator<'a>,
	headers: &'a BlockHeaderProvider,
	consensus: &'a ConsensusParams,
	last_state: ThresholdState,
}

impl<'a> ThresholdIterator<'a> {
	fn new(deployment: Deployment, headers: &'a BlockHeaderProvider, to_check: u32, consensus: &'a ConsensusParams, state: ThresholdState) -> Self {
		ThresholdIterator {
			deployment: deployment,
			block_iterator: BlockIterator::new(to_check, consensus.miner_confirmation_window, headers),
			headers: headers,
			consensus: consensus,
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

		let median = median_timestamp(&header, self.headers);

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
					let count = count_deployment_matches(block_number, self.headers, self.deployment, self.consensus.miner_confirmation_window);
					if count >= self.consensus.rule_change_activation_threshold as usize {
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
			block_hash: header.hash(),
			state: self.last_state,
		};

		Some(result)
	}
}

#[cfg(test)]
mod tests {
	use super::first_of_the_period;

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
}
