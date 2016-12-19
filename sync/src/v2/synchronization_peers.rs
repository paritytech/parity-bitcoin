use std::collections::HashMap;
use parking_lot::RwLock;
use chain::{IndexedBlock, IndexedTransaction};
use message::types;
use p2p::OutboundSyncConnectionRef;
use synchronization_filter::Filter;
use types::{PeerIndex, ConnectionRef};

/// Block announcement type
pub enum BlockAnnouncementType {
	/// Send inventory message with block hash [default behavior]
	SendInventory,
	/// Send headers message with block header
	SendHeaders,
	/// Send cmpctblock message with this block
	SendCompactBlock,
	/// Do not announce blocks at all
	DoNotAnnounce,
}

/// Transaction announcement type
pub enum TransactionAnnouncementType {
	/// Send inventory message with transaction hash [default behavior]
	SendInventory,
	/// Do not announce transactions at all
	DoNotAnnounce,
}

/// Connected peers
pub trait Peers : PeersContainer + PeersFilters + PeersOptions {
	/// Get peer connection
	fn connection(&self, peer_index: PeerIndex) -> Option<OutboundSyncConnectionRef>;
}

/// Connected peers container
pub trait PeersContainer {
	/// Insert new peer connection
	fn insert(&self, peer_index: PeerIndex, connection: OutboundSyncConnectionRef);
	/// Remove peer connection
	fn remove(&self, peer_index: PeerIndex);
	/// Close and remove peer connection due to misbehaving
	fn misbehaving(&self, peer_index: PeerIndex, reason: &str);
	/// Close and remove peer connection due to detected DOS attempt
	fn dos(&self, peer_index: PeerIndex, reason: &str);
}

/// Filters for peers connections
pub trait PeersFilters {
	/// Set up bloom filter for the connection
	fn set_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterLoad);
	/// Update bloom filter for the connection
	fn update_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterAdd);
	/// Clear bloom filter for the connection
	fn clear_bloom_filter(&self, peer_index: PeerIndex);
	/// Set up fee filter for the connection
	fn set_fee_filter(&self, peer_index: PeerIndex, filter: types::FeeFilter);
	/// Is block passing filters for the connection
	fn filter_block(&self, peer_index: PeerIndex, block: &IndexedBlock);
	/// Is block passing filters for the connection
	fn filter_transaction(&self, peer_index: PeerIndex, transaction: &IndexedTransaction);
}

/// Options for peers connections
pub trait PeersOptions {
	/// Set up new block announcement type for the connection
	fn set_block_announcement_type(&self, peer_index: PeerIndex, announcement_type: BlockAnnouncementType);
	/// Set up new transaction announcement type for the connection
	fn set_transaction_announcement_type(&self, peer_index: PeerIndex, announcement_type: TransactionAnnouncementType);
	/// Get block announcement type for the connection
	fn block_announcement_type(&self, peer_index: PeerIndex) -> BlockAnnouncementType;
	/// Get transaction announcement type for the connection
	fn transaction_announcement_type(&self, peer_index: PeerIndex) -> TransactionAnnouncementType;
}

/// Single connected peer data
struct Peer {
	/// Connection to this peer
	pub connection: OutboundSyncConnectionRef,
	/// Connection filter
	pub filter: Filter,
	/// Block announcement type
	pub block_announcement_type: BlockAnnouncementType,
	/// Transaction announcement type
	pub transaction_announcement_type: TransactionAnnouncementType,
}

/// Default implementation of connectd peers container
struct PeersImpl {
	/// All connected peers. Most of times this field is accessed, it is accessed in read mode.
	/// So this lock shouldn't be a performance problem.
	peers: RwLock<HashMap<PeerIndex, Peer>>,
}

impl Peers for PeersImpl {
	fn connection(&self, peer_index: PeerIndex) -> Option<OutboundSyncConnectionRef> {
		self.peers.read(peer_index).get(peer_index).cloned()
	}
}

impl PeersContainer for PeersImpl {
	fn insert(&self, peer_index: PeerIndex, connection: OutboundSyncConnectionRef) {
		trace!(target: "sync", "Connected to peer#{}", peer_index);
		assert!(self.peers.write().insert(peer_index, ConnectionRef::new(connection)).is_none());
	}

	fn remove(&self, peer_index: PeerIndex) {
		if self.peers.write().remove(peer_index).is_some() {
			trace!(target: "sync", "Disconnected from peer#{}", peer_index);
		}
	}

	fn misbehaving(&self, peer_index: PeerIndex, reason: &str) {
		if let Some(peer) = self.peers.write().remove(peer_index) {
			trace!(target: "sync", "Disconnecting from peer#{} due to misbehaving: {}", peer_index, reason);
			peer.connection.close();
		}
	}

	fn dos(&self, peer_index: PeerIndex, reason: &str) {
		if let Some(peer) = self.peers.write().remove(peer_index) {
			trace!(target: "sync", "Disconnecting from peer#{} due to DOS: {}", peer_index, reason);
			peer.connection.close();
		}
	}
}

impl PeersFilters for PeersImpl {
	fn set_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterLoad) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.filter.set_bloom_filter(filter);
		}
	}

	fn update_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterAdd) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.filter.add_bloom_filter(filter);
		}
	}

	fn clear_bloom_filter(&self, peer_index: PeerIndex) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.filter.clear_bloom_filter();
		}
	}

	fn set_fee_filter(&self, peer_index: PeerIndex, filter: types::FeeFilter) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.filter.set_fee_filter(filter.fee_rate);
		}
	}

	fn filter_block(&self, peer_index: PeerIndex, block: &IndexedBlock) {
		if let Some(peer) = self.peers.read().get() {
			peer.filter.filter_block(block);
		}
	}

	fn filter_transaction(&self, peer_index: PeerIndex, transaction: &IndexedTransaction) {
		if let Some(peer) = self.peers.read().get() {
			peer.filter.filter_transaction(transaction);
		}
	}
}

impl PeersOptions for PeersImpl {
	fn set_block_announcement_type(&self, peer_index: PeerIndex, announcement_type: BlockAnnouncementType) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.block_announcement_type = announcement_type;
		}
	}

	fn set_transaction_announcement_type(&self, peer_index: PeerIndex, announcement_type: TransactionAnnouncementType) {
		if let Some(peer) = self.peers.write().get_mut(peer_index) {
			peer.transaction_announcement_type = announcement_type;
		}
	}

	fn block_announcement_type(&self, peer_index: PeerIndex) -> BlockAnnouncementType {
		self.peers.read()
			.get(peer_index)
			.map(|peer| peer.block_announcement_type)
			.unwrap_or(BlockAnnouncementType::DoNotAnnounce)
	}

	fn transaction_announcement_type(&self, peer_index: PeerIndex) -> TransactionAnnouncementType {
		self.peers.read()
			.get(peer_index)
			.map(|peer| peer.transaction_announcement_type)
			.unwrap_or(TransactionAnnouncementType::DoNotAnnounce)
	}
}
