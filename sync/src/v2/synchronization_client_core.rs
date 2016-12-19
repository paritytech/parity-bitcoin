use std::cmp::{min, max};
use std::collections::{VecDeque, HashSet};
use std::mem::swap;
use time;
use chain::{IndexedBlock, IndexedTransaction};
use message::common;
use message::types;
use primitives::hash::H256;
use synchronization_blocks_queue::{BlocksQueue, BlockState};
use synchronization_executor::Task;
use synchronization_verifier::{VerificationTask, VerificationTasks};
use types::{PeerIndex};

// TODO: refactor DeadEnd blocks

/// Approximate maximal number of blocks hashes in scheduled queue.
const MAX_SCHEDULED_HASHES: u32 = 4 * 1024;
/// Approximate maximal number of blocks hashes in requested queue.
const MAX_REQUESTED_BLOCKS: u32 = 256;
/// Approximate maximal number of blocks in verifying queue.
const MAX_VERIFYING_BLOCKS: u32 = 256;
/// Minimum number of blocks to request from peer
const MIN_BLOCKS_IN_REQUEST: u32 = 32;
/// Maximum number of blocks to request from peer
const MAX_BLOCKS_IN_REQUEST: u32 = 128;
/// Number of blocks to receive since synchronization start to begin duplicating blocks requests
const NEAR_EMPTY_VERIFICATION_QUEUE_THRESHOLD_BLOCKS: usize = 20;
/// Number of seconds left before verification queue will be empty to count it as 'near empty queue'
const NEAR_EMPTY_VERIFICATION_QUEUE_THRESHOLD_S: f64 = 20_f64;
/// Number of blocks to inspect when calculating average sync speed
const SYNC_SPEED_BLOCKS_TO_INSPECT: usize = 512;
/// Number of blocks to inspect when calculating average blocks speed
const BLOCKS_SPEED_BLOCKS_TO_INSPECT: usize = 512;

/// Synchronization client core config
pub struct ClientCoreConfig {
	/// Receiving dead-end block from peer leads to disconnect
	pub dead_end_fatal: bool,
}

/// Synchronization client core
pub trait ClientCore {
	/// Called upon receiving inventory message
	fn on_inventory(&mut self, peer_index: PeerIndex, message: types::Inv);
	/// Called upon receiving headers message
	fn on_headers(&mut self, peer_index: PeerIndex, message: types::Headers);
	/// Called upon receiving notfound message
	fn on_notfound(&mut self, peer_index: PeerIndex, message: types::NotFound);
	/// Called upon receiving block message
	fn on_block(&mut self, peer_index: PeerIndex, block: IndexedBlock) -> Option<VerificationTasks>;
	/// Called upon receiving transaction message
	fn on_transaction(&mut self, peer_index: PeerIndex, transaction: IndexedTransaction, relay: bool) -> Option<VerificationTasks>;
	/// Called when connection is closed
	fn on_disconnect(&mut self, peer_index: PeerIndex);
	/// Try switch to saturated state
	fn try_switch_to_saturated_state(&mut self) -> bool;
	/// Execute pending synchronization tasks
	fn execute_synchronization_tasks(&mut self, forced_blocks_requests: Option<Vec<H256>>, final_blocks_requests: Option<Vec<H256>>);
}

/// Synchronization state
#[derive(Debug, Clone, Copy)]
enum State {
	/// We know that there are > 1 unknown blocks, unknown to us in the blockchain
	Synchronizing(f64, u32),
	/// There is only one unknown block in the blockchain
	NearlySaturated,
	/// We have downloaded all blocks of the blockchain of which we have ever heard
	Saturated,
}

/// Synchronization client core implementation
pub struct ClientCoreImpl {
	/// Synchronization state
	state: State,
	/// Synchronization peers
	peers: PeersRef,
	/// Synchronization peers tasks
	peers_tasks: PeersTasks,
	/// Transactions queue
	tx_queue: TransactionsQueue,
	/// Blocks queue
	blocks_queue: BlocksQueue,
	/// Chain verifier
	chain_verifier: ChainVerifier,
	/// Core stats
	stats: Stats,
}

/// Synchronization client core stats
struct Stats {
	/// Block processing speed meter
	block_speed_meter: AverageSpeedMeter,
	/// Block synchronization speed meter
	sync_speed_meter: AverageSpeedMeter,
}

/// Transaction append error
enum AppendTransactionError {
	/// Cannot append when synchronization is in progress
	Synchronizing,
	/// Some inputs of transaction are unknown
	Orphan(HashSet<H256>),
}

impl ClientCore for ClientCoreImpl {
	fn on_inventory(&mut self, peer_index: PeerIndex, message: types::Inv) {
		// we are synchronizing => we ask only for blocks with known headers => there are no useful blocks hashes for us
		// we are synchronizing => we ignore all transactions until it is completed => there are no useful transactions hashes for us
		if self.state.is_synchronizing() {
			return;
		}

		// else ask for all unknown transactions and blocks
		let inventory: Vec<_> = message.inventory.into_iter()
			.filter(|item| {
				match item.inv_type {
					InventoryType::Tx => self.tx_queue.state(&item.hash) == TransactionState::Unknown,
					InventoryType::Block => match self.blocks_queue.state(&item.hash) {
						BlockState::Unknown => true,
						BlockState::DeadEnd if self.config.dead_end_fatal => {
							self.peers.misbehaving(peer_index, format!("Provided dead-end block {:?}", item.hash.to_reversed_str()));
							return false;
						},
						_ => false,
					},
					_ => false,
				}
			})
			.collect();
		if inventory.is_empty() {
			return;
		}

		let message = types::GetData::with_inventory(inventory);
		self.executor.execute(Task::GetData(peer_index, message));
	}

	fn on_headers(&mut self, peer_index: PeerIndex, message: types::Headers) {
		assert!(!message.headers.is_empty(), "This must be checked in incoming connection");

		// transform to indexed headers
		let headers: Vec<_> = message.headers.into_iter().map(IndexedBlockHeader::from).collect();

		// update peers to select next tasks
		self.peers_tasks.on_headers_received(peer_index);

		// headers are ordered
		// => if we know nothing about headers[0].parent
		// => all headers are also unknown to us
		let header0 = &headers[0].raw;
		if self.blocks_queue.state(&header0.previous_header_hash) == BlockState::Unknown {
			warn!(target: "sync", "Previous header of the first header from peer#{} `headers` message is unknown. First: {}. Previous: {}", peer_index, header0.hash().to_reversed_str(), header0.previous_header_hash.to_reversed_str());
			return;
		}

		// find first unknown header position
		// optimization: normally, the first header will be unknown
		let num_headers = headers.len();
		let first_unknown_index = match self.blocks_queue.state(&header0.hash) {
			BlockState::Unknown => 0,
			_ => {
				// optimization: if last header is known, then all headers are also known
				let header_last = &headers[num_headers - 1].raw;
				match self.blocks_queue.state(&header_last.hash) {
					BlockState::Unknown => 1 + headers.iter().skip(1)
						.position(|header| self.blocks_queue.state(&header.hash) == BlockState::Unknown)
						.expect("last header has UnknownState; we are searching for first unknown header; qed"),
					// else all headers are known
					_ => {
						trace!(target: "sync", "Ignoring {} known headers from peer#{}", headers.len(), peer_index);
						// but this peer is still useful for synchronization
						self.peers_tasks.mark_useful(peer_index);
						return;
					},
				}
			}
		};

		// if first unknown header is preceeded with dead end header => disconnect
		let mut last_known_hash = if first_unknown_index > 0 { &headers[first_unknown_index - 1].hash } else { &header0.raw.previous_header_hash };
		if self.config.dead_end_fatal {
			if self.blocks_queue.state(last_known_hash) == BlockState::DeadEnd {
				self.peers.misbehaving(peer_index, format!("Provided dead-end block {:?}", last_known_hash.to_reversed_str()));
				return;
			} else {
				warn!(target: "sync", "Peer#{} has provided us with dead-end block {:?}", peer_index, block.hash.to_reversed_str());
			}
		}

		// validate blocks headers before scheduling
		let mut headers_provider = InMemoryHeadersProvider::new(&chain);
		for header in &headers[first_unknown_index..num_headers] {
			// check that this header is direct child of previous header
			if header.previous_header_hash != last_known_hash {
				self.peers.misbehaving(peer_index, format!("Neighbour headers in peer#{} `headers` message are unlinked: Prev: {}, PrevLink: {}, Curr: {}", peer_index, prev_block_hash.to_reversed_str(), block_header.previous_header_hash.to_reversed_str(), block_header_hash.to_reversed_str()));
				return;
			}

			// check that we do not know all blocks in range [first_unknown_index..]
			// if we know some block => there has been verification error => all headers should be ignored
			// see when_previous_block_verification_failed_fork_is_not_requested for details
			match self.blocks_queue.state(&header.raw) {
				BlockState::Unknown => (),
				BlockState::DeadEnd if self.config.dead_end_fatal => {
					self.peers.misbehaving(peer_index, format!("Provided dead-end block {:?}", header.raw.to_reversed_str()));
					return;
				},
				_ => {
					trace!(target: "sync", "Ignoring {} headers from peer#{} - known header in the middle", message.headers.len(), peer_index);
					self.peers_tasks.mark_useful(peer_index);
					return;
				},
			}

			// verify header
			if self.verify_headers {
				if let Err(error) = self.chain_verifier.verify_block_header(&headers_provider, &header.hash, &header.raw) {
					if self.config.dead_end_fatal {
						self.peers.misbehaving(peer_index, format!("Neighbour headers in peer#{} `headers` message are unlinked: Prev: {}, PrevLink: {}, Curr: {}", peer_index, prev_block_hash.to_reversed_str(), block_header.previous_header_hash.to_reversed_str(), block_header_hash.to_reversed_str()));
					} else {
						warn!(target: "sync", "Neighbour headers in peer#{} `headers` message are unlinked: Prev: {}, PrevLink: {}, Curr: {}", peer_index, prev_block_hash.to_reversed_str(), block_header.previous_header_hash.to_reversed_str(), block_header_hash.to_reversed_str());
					}
					self.blocks_queue.dead_end_block(&header.hash);
					return;
				}
			}

			last_known_hash = &header.hash;
			headers_provider.append_header(header);
		}

		// append headers to the queue
		trace!(target: "sync", "New {} headers from peer#{}. First {:?}, last: {:?}",
			num_headers - first_unknown_index,
			peer_index,
			headers[0].hash.to_reversed_str(),
			headers[num_headers - 1].hash.to_reversed_str()
		);
		self.blocks_queue.schedule_headers(&headers[first_unknown_index..num_headers]);

		// this peers has supplied us with new headers => useful indeed
		self.peers_tasks.mark_useful(peer_index);
		// and execute tasks
		self.execute_synchronization_tasks(None, None);
	}

	fn on_notfound(&mut self, peer_index: PeerIndex, message: types::NotFound) {
		let notfound_blocks: HashSet<_> = message.inventory.into_iter()
			.filter(|item| item.type == common::InventoryType::Block)
			.map(|item| item.hash)
			.collect();
		if notfound_blocks.is_empty() {
			// it is not about blocks => just ignore it
			return;
		}

		if let Some(blocks_tasks) = self.peers_tasks.blocks_tasks(peer_index) {
			// check if peer has responded with notfound to requested blocks
			if blocks_tasks.intersection(&notfound_blocks).nth(0).is_none() {
				// if notfound some other blocks => just ignore the message
				return;
			}

			// for now, let's exclude peer from synchronization - we are relying on full nodes for synchronization
			trace!(target: "sync", "Peer#{} is excluded from synchronization as it has not found blocks", peer_index);
			let removed_tasks = self.peers_tasks.reset_blocks_tasks(peer_index);
			self.peers_tasks.mark_unuseful(peer_index);

			// if peer has had some blocks tasks, rerequest these blocks
			self.execute_synchronization_tasks(Some(removed_tasks), None);
		}
	}

	fn on_block(&mut self, peer_index: PeerIndex, block: IndexedBlock) -> Option<VerificationTasks> {
		// update peers to select next tasks
		self.peers_tasks.on_block_received(peer_index, &block.hash);

		// prepare list of blocks to verify + make all required changes to the chain
		let block_state = self.blocks_queue.state(&block.hash);
		match block_state {
			BlockState::Verifying | BlockState::Stored => {
				// remember peer as useful
				self.peers_tasks.mark_useful(peer_index);
				// already verifying || stored => no new verification tasks
				None
			},
			BlockState::Unknown | BlockState::Scheduled | BlockState::Requested | BlockState::DeadEnd => {
				// if configured => drop connection on dead-end block
				if block_state == BlockState::DeadEnd {
					if self.config.close_connection_on_bad_block {
						self.peers.misbehaving(peer_index, format!("Provided dead-end block {:?}", block.hash.to_reversed_str()));
						return None;
					} else {
						warn!(target: "sync", "Peer#{} has provided us with dead-end block {:?}", peer_index, block.hash.to_reversed_str());
					}
				}

				// new block received => update synchronization speed
				self.stats.sync_speed_meter.checkpoint();

				// check parent block state
				let parent_block_state = self.blocks_queue.state(&block.header.raw.previous_header_hash);
				match parent_block_state {
					BlockState::Unknown | BlockState::DeadEnd => {
						// if configured => drop connection on dead-end block
						if parent_block_state == BlockState::DeadEnd {
							if self.config.close_connection_on_bad_block {
								self.peers.misbehaving(peer_index, format!("Provided dead-end block {:?}", block.hash.to_reversed_str()));
								return None;
							} else {
								warn!(target: "sync", "Peer#{} has provided us with after-dead-end block {:?}", peer_index, block.hash.to_reversed_str());
							}
						}

						if self.state.is_synchronizing() {
							// when synchronizing, we tend to receive all blocks in-order
							trace!(
								target: "sync",
								"Ignoring block {} from peer#{}, because its parent is unknown and we are synchronizing",
								block.hash.to_reversed_str(),
								peer_index
							);
							// remove block from current queue
							self.blocks_queue.forget(&block.hash);
							// remove orphaned blocks
							for block in self.orphaned_blocks_pool.remove_blocks_for_parent(&block.hash) {
								self.blocks_queue.forget(&b.hash);
							}
						} else {
							// remove this block from the queue
							self.blocks_queue.forget(&block.hash);
							// remember this block as unknown
							if !self.orphaned_blocks_pool.contains_unknown_block(&block.hash) {
								self.orphaned_blocks_pool.insert_unknown_block(block);
							}
						}

						// no verification tasks, as we have either ignored, or postponed verification
						None
					},
					BlockState::Verifying | BlockState::Stored => {
						// remember peer as useful
						self.peers_tasks.useful_peer(peer_index);
						// forget blocks we are going to verify
						// + remember that we are verifying these blocks
						// + remember that we are verifying these blocks by message from this peer
						let orphan_blocks = self.blocks_queue.remove_blocks_for_parent(&block.hash);
						self.blocks_queue.forget_leave_header(&block.hash);
						self.blocks_queue.verify(&block.hash);
						//self.verifying_blocks_by_peer.insert(block.hash.clone(), peer_index);
						for orphan_block in &orphan_blocks {
							self.blocks_queue.forget_leave_header(&orphan_block.hash);
							self.blocks_queue.verify(&orphan_block.hash);
							//self.verifying_blocks_by_peer.insert(orphan_block.hash.clone(), peer_index);
						}
						// update
						/*match self.verifying_blocks_futures.entry(peer_index) {
							Entry::Occupied(mut entry) => {
								entry.get_mut().0.extend(blocks_to_verify.iter().map(|&(ref h, _)| h.clone()));
							},
							Entry::Vacant(entry) => {
								let block_hashes: HashSet<_> = blocks_to_verify.iter().map(|&(ref h, _)| h.clone()).collect();
								entry.insert((block_hashes, Vec::new()));
							}
						}*/
						// schedule verification of this block && all dependent orphans
						let mut verification_tasks: VecDeque<_> = VecDeque::with_capacity(orphan_blocks.len() + 1);
						verification_tasks.push_back(VerificationTask::VerifyBlock(block));
						verification_tasks.extend(orphan_blocks.into_iter().map(|block| VerificationTask::VerifyBlock(block)));

						// we have blocks to verify
						Some(verification_tasks);
					},
					BlockState::Requested | BlockState::Scheduled => {
						// remember peer as useful
						self.peers_tasks.mark_useful(peer_index);
						// remember as orphan block
						self.blocks_queue.insert_orphaned_block(block);

						// no new blocks to verify
						None
					}
				}
			},
		}
	}

	fn on_transaction(&mut self, peer_index: PeerIndex, transaction: IndexedTransaction, relay: bool) -> Option<VerificationTasks> {
		match self.try_append_transaction(transaction, relay) {
			Err(AppendTransactionError::Orphan(transaction, unknown_parents)) => {
				self.orphaned_transactions_pool.insert(transaction, unknown_parents);
				None
			},
			Err(AppendTransactionError::Synchronizing) => None,
			Ok(transactions) => Some(transactions.into_iter().map(|transaction| VerificationTask::VerifyTransaction(transaction)).collect()),
		}
	}

	fn on_disconnect(&mut self, peer_index: PeerIndex) {
		// when last peer is disconnected, reset, but let verifying blocks be verified
		let peer_tasks = self.peers_tasks.on_peer_disconnected(peer_index);
		if !self.peers_tasks.has_any_useful() {
			self.switch_to_saturated_state();
		} else if peer_tasks.is_some() {
			self.execute_synchronization_tasks(peer_tasks, None);
		}
	}

	fn try_switch_to_saturated_state(&mut self) -> bool {
		// move to saturated state if there are no more blocks in scheduled || requested state
		let in_saturated_state = self.blocks_chain.state_len(BlockState::Scheduled) != 0
			|| self.blocks_chain.state_len(BlockState::Requested) != 0;
		if in_saturated_state {
			self.switch_to_saturated_state();
		}
		in_saturated_state
	}

	fn execute_synchronization_tasks(&mut self, forced_blocks_requests: Option<Vec<H256>>, final_blocks_requests: Option<Vec<H256>>) {
		let mut tasks: Vec<Task> = Vec::new();

		// display information if processed many blocks || enough time has passed since sync start
		self.print_synchronization_information();

		// if some blocks requests are forced => we should ask peers even if there are no idle peers
		if let Some(forced_blocks_requests) = forced_blocks_requests {
			let useful_peers = self.peers_tasks.useful_peers();
			// if we have to request blocks && there are no useful peers at all => switch to saturated state
			if useful_peers.is_empty() {
				warn!(target: "sync", "Last peer was marked as non-useful. Moving to saturated state.");
				self.switch_to_saturated_state();
				return;
			}

			let forced_tasks = self.prepare_blocks_requests_tasks(useful_peers, forced_blocks_requests);
			tasks.extend(forced_tasks);
		}

		// if some blocks requests are marked as last [i.e. blocks are potentialy wrong] => ask peers anyway
		if let Some(final_blocks_requests) = final_blocks_requests {
			let useful_peers = self.peers.useful_peers();
			if !useful_peers.is_empty() { // if empty => not a problem, just forget these blocks
				let forced_tasks = self.prepare_blocks_requests_tasks(useful_peers, final_blocks_requests);
				tasks.extend(forced_tasks);
			}
		}

		let mut blocks_requests: Option<Vec<H256>> = None;
		let blocks_idle_peers = self.peers.idle_peers_for_blocks();
		{
			// check if we can query some blocks hashes
			let inventory_idle_peers = self.peers.idle_peers_for_inventory();
			if !inventory_idle_peers.is_empty() {
				let scheduled_hashes_len = self.chain.state_len(BlockState::Scheduled);
				if scheduled_hashes_len < MAX_SCHEDULED_HASHES {
					for inventory_peer in &inventory_idle_peers {
						self.peers.on_inventory_requested(*inventory_peer);
					}

					let inventory_tasks = inventory_idle_peers.into_iter().map(Task::RequestBlocksHeaders);
					tasks.extend(inventory_tasks);
				}
			}

			let blocks_idle_peers_len = blocks_idle_peers.len() as u32;
			if blocks_idle_peers_len != 0 {
				// check if verification queue is empty/almost empty
				// && there are pending blocks requests
				// && there are idle block peers
				// => we may need to duplicate pending blocks requests to idle peers
				// this will result in additional network load, but verification queue will be filled up earlier
				// it is very useful when dealing with large blocks + some peer is responding, but with very low speed:
				//    requested: [B1, B2, B3, B4] from peer1
				//    orphans: [B5, B6, B7, B8, ... B1024] ===> 1GB of RAM
				//    verifying: None <=== we are waiting for B1 to come
				//    idle: [peer2]
				//    peer1 responds with single block in ~20 seconds
				//    => we could ask idle peer2 about [B1, B2, B3, B4]
				// these requests has priority over new blocks requests below
				let requested_hashes_len = self.blocks_chain.length_of_blocks_state(BlockState::Requested);
				let verifying_hashes_len = self.blocks_chain.length_of_blocks_state(BlockState::Verifying);
				if requested_hashes_len != 0 {
					let verification_speed: f64 = self.block_speed_meter.speed();
					let synchronization_speed: f64 = self.sync_speed_meter.speed();
					// estimate time when verification queue will be empty
					let verification_queue_will_be_empty_in = if verifying_hashes_len == 0 {
						// verification queue is already empty
						if self.block_speed_meter.inspected_items_len() < NEAR_EMPTY_VERIFICATION_QUEUE_THRESHOLD_BLOCKS {
							// the very beginning of synchronization
							// => peers have not yet responded with a single requested blocks
							60_f64
						} else {
							// blocks were are already received
							// => bad situation
							0_f64
						}
					} else {
						if verification_speed < 0.01_f64 {
							// verification speed is too slow
							60_f64
						} else {
							// blocks / (blocks / second) -> second
							verifying_hashes_len as f64 / verification_speed
						}
					};
					// estimate time when all synchronization requests will complete
					let synchronization_queue_will_be_full_in = if synchronization_speed < 0.01_f64 {
						// synchronization speed is too slow
						60_f64
					} else {
						// blocks / (blocks / second) -> second
						requested_hashes_len as f64 / synchronization_speed
					};
					// if verification queue will be empty before all synchronization requests will be completed
					// + do not spam with duplicated blocks requests if blocks are too big && there are still blocks left for NEAR_EMPTY_VERIFICATION_QUEUE_THRESHOLD_S
					// => duplicate blocks requests
					if synchronization_queue_will_be_full_in > verification_queue_will_be_empty_in &&
						verification_queue_will_be_empty_in < NEAR_EMPTY_VERIFICATION_QUEUE_THRESHOLD_S {
						// blocks / second * second -> blocks
						let hashes_requests_to_duplicate_len = synchronization_speed * (synchronization_queue_will_be_full_in - verification_queue_will_be_empty_in);
						// do not ask for too many blocks
						let hashes_requests_to_duplicate_len = min(MAX_BLOCKS_IN_REQUEST, hashes_requests_to_duplicate_len as u32);
						// ask for at least 1 block
						let hashes_requests_to_duplicate_len = max(1, min(requested_hashes_len, hashes_requests_to_duplicate_len));
						blocks_requests = Some(self.blocks_chain.best_n_of_blocks_state(BlockState::Requested, hashes_requests_to_duplicate_len));

						trace!(target: "sync", "Duplicating {} blocks requests. Sync speed: {} * {}, blocks speed: {} * {}.", hashes_requests_to_duplicate_len, synchronization_speed, requested_hashes_len, verification_speed, verifying_hashes_len);
					}
				}

				// check if we can move some blocks from scheduled to requested queue
				{
					let scheduled_hashes_len = self.blocks_chain.length_of_blocks_state(BlockState::Scheduled);
					if requested_hashes_len + verifying_hashes_len < MAX_REQUESTED_BLOCKS + MAX_VERIFYING_BLOCKS && scheduled_hashes_len != 0 {
						let chunk_size = min(MAX_BLOCKS_IN_REQUEST, max(scheduled_hashes_len / blocks_idle_peers_len, MIN_BLOCKS_IN_REQUEST));
						let hashes_to_request_len = chunk_size * blocks_idle_peers_len;
						let hashes_to_request = self.blocks_chain.request_blocks_hashes(hashes_to_request_len);
						match blocks_requests {
							Some(ref mut blocks_requests) => blocks_requests.extend(hashes_to_request),
							None => blocks_requests = Some(hashes_to_request),
						}
					}
				}
			}
		}

		// append blocks requests tasks
		if let Some(blocks_requests) = blocks_requests {
			tasks.extend(self.prepare_blocks_requests_tasks(blocks_idle_peers, blocks_requests));
		}

		// execute synchronization tasks
		for task in tasks {
			self.executor.execute(task);
		}
	}
}

impl ClientCoreImpl {
	/// Print synchronization information
	fn print_synchronization_information(&mut self) {
		if let State::Synchronizing(timestamp, num_of_blocks) = self.state {
			let new_timestamp = time::precise_time_s();
			let timestamp_diff = new_timestamp - timestamp;
			let new_num_of_blocks = self.blocks_queue.best_storage_block().number;
			let blocks_diff = if new_num_of_blocks > num_of_blocks { new_num_of_blocks - num_of_blocks } else { 0 };
			if timestamp_diff >= 60.0 || blocks_diff > 1000 {
				self.state = State::Synchronizing(time::precise_time_s(), new_num_of_blocks);

				info!(target: "sync", "{:?} @ Processed {} blocks in {} seconds. Queue information: {:?}"
					, time::strftime("%H:%M:%S", &time::now()).unwrap()
					, blocks_diff, timestamp_diff
					, self.blocks_queue.information());
			}
		}
	}

	/// Switch to saturated state
	fn switch_to_saturated_state(&mut self) {
		if self.state.is_saturated() {
			return;
		}

		self.state = State::Saturated;
		self.peers.reset();

		// remove sync orphans, but leave unknown orphans until they'll be removed by management thread
		let removed_orphans = self.orphaned_blocks_pool.remove_known_blocks();

		// leave currently verifying blocks
		{
			self.chain.forget_blocks(&removed_orphans);
			self.chain.forget_all_blocks_with_state(BlockState::Requested);
			self.chain.forget_all_blocks_with_state(BlockState::Scheduled);

			use time;
			info!(target: "sync", "{:?} @ Switched to saturated state. Chain information: {:?}",
				time::strftime("%H:%M:%S", &time::now()).unwrap(),
				self.chain.information());
		}

		// finally - ask all known peers for their best blocks inventory, in case if some peer
		// has lead us to the fork
		// + ask all peers for their memory pool
		for peer in self.peers_tasks.all_peers() {
			self.executor.execute(Task::RequestBlocksHeaders(peer));
			self.executor.execute(Task::RequestMemoryPool(peer));
		}
	}

	/// Prepare blocks requests for peers
	fn prepare_blocks_requests_tasks(&mut self, peers: Vec<PeerIndex>, mut hashes: Vec<H256>) -> Vec<Task> {
		// TODO: ask most fast peers for hashes at the beginning of `hashes`
		let chunk_size = min(MAX_BLOCKS_IN_REQUEST, max(hashes.len() as u32, MIN_BLOCKS_IN_REQUEST));
		let last_peer_index = peers.len() - 1;
		let mut tasks: Vec<Task> = Vec::new();
		for (peer_index, peer) in peers.into_iter().enumerate() {
			// we have to request all blocks => we will request last peer for all remaining blocks
			let peer_chunk_size = if peer_index == last_peer_index { hashes.len() } else { min(hashes.len(), chunk_size as usize) };
			if peer_chunk_size == 0 {
				break;
			}

			let mut chunk_hashes = hashes.split_off(peer_chunk_size);
			swap(&mut chunk_hashes, &mut hashes);

			self.peers.on_blocks_requested(peer, &chunk_hashes);
			tasks.push(Task::RequestBlocks(peer, chunk_hashes));
		}

		assert_eq!(hashes.len(), 0);
		tasks
	}
}
