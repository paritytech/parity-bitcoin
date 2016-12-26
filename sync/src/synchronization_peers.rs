use std::collections::HashMap;
use parking_lot::RwLock;
use chain::{IndexedBlock, IndexedTransaction};
use message::types;
use p2p::OutboundSyncConnectionRef;
use primitives::hash::H256;
use types::PeerIndex;
use utils::{KnownHashType, ConnectionFilter};

/// Block announcement type
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Copy)]
pub enum TransactionAnnouncementType {
	/// Send inventory message with transaction hash [default behavior]
	SendInventory,
	/// Do not announce transactions at all
	DoNotAnnounce,
}

/// `merkleblock` build artefacts
#[derive(Debug, PartialEq)]
pub struct MerkleBlockArtefacts {
	/// `merkleblock` message
	pub merkleblock: types::MerkleBlock,
	/// All matching transactions
	pub matching_transactions: Vec<IndexedTransaction>,
}

/// Connected peers
pub trait Peers : Send + Sync + PeersContainer + PeersFilters + PeersOptions {
	/// Get peer connection
	fn connection(&self, peer_index: PeerIndex) -> Option<OutboundSyncConnectionRef>;
}

/// Connected peers container
pub trait PeersContainer {
	/// Enumerate all known peers (TODO: iterator + separate entity 'Peer')
	fn enumerate(&self) -> Vec<PeerIndex>;
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
	fn filter_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> BlockAnnouncementType;
	/// Is block passing filters for the connection
	fn filter_transaction(&self, peer_index: PeerIndex, transaction: &IndexedTransaction, transaction_fee_rate: Option<u64>) -> TransactionAnnouncementType;
	/// Remember known hash
	fn hash_known_as(&self, peer_index: PeerIndex, hash: H256, hash_type: KnownHashType);
	/// Is given hash known by peer as hash of given type
	fn is_hash_known_as(&self, peer_index: PeerIndex, hash: &H256, hash_type: KnownHashType) -> bool;
	/// Build compact block using filter for given peer
	fn build_compact_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> Option<types::CompactBlock>;
	/// Build merkle block using filter for given peer
	fn build_merkle_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> Option<MerkleBlockArtefacts>;
}

/// Options for peers connections
pub trait PeersOptions {
	/// Set up new block announcement type for the connection
	fn set_block_announcement_type(&self, peer_index: PeerIndex, announcement_type: BlockAnnouncementType);
	/// Set up new transaction announcement type for the connection
	fn set_transaction_announcement_type(&self, peer_index: PeerIndex, announcement_type: TransactionAnnouncementType);
}

/// Single connected peer data
struct Peer {
	/// Connection to this peer
	pub connection: OutboundSyncConnectionRef,
	/// Connection filter
	pub filter: ConnectionFilter,
	/// Block announcement type
	pub block_announcement_type: BlockAnnouncementType,
	/// Transaction announcement type
	pub transaction_announcement_type: TransactionAnnouncementType,
}

/// Default implementation of connectd peers container
#[derive(Default)]
pub struct PeersImpl {
	/// All connected peers. Most of times this field is accessed, it is accessed in read mode.
	/// So this lock shouldn't be a performance problem.
	peers: RwLock<HashMap<PeerIndex, Peer>>,
}

impl Peer {
	pub fn with_connection(connection: OutboundSyncConnectionRef) -> Self {
		Peer {
			connection: connection,
			filter: ConnectionFilter::default(),
			block_announcement_type: BlockAnnouncementType::SendInventory,
			transaction_announcement_type: TransactionAnnouncementType::SendInventory,
		}
	}
}

impl Peers for PeersImpl {
	fn connection(&self, peer_index: PeerIndex) -> Option<OutboundSyncConnectionRef> {
		self.peers.read().get(&peer_index).map(|peer| peer.connection.clone())
	}
}

impl PeersContainer for PeersImpl {
	fn enumerate(&self) -> Vec<PeerIndex> {
		self.peers.read().keys().cloned().collect()
	}

	fn insert(&self, peer_index: PeerIndex, connection: OutboundSyncConnectionRef) {
		trace!(target: "sync", "Connected to peer#{}", peer_index);
		assert!(self.peers.write().insert(peer_index, Peer::with_connection(connection)).is_none());
	}

	fn remove(&self, peer_index: PeerIndex) {
		if self.peers.write().remove(&peer_index).is_some() {
			trace!(target: "sync", "Disconnected from peer#{}", peer_index);
		}
	}

	fn misbehaving(&self, peer_index: PeerIndex, reason: &str) {
		if let Some(peer) = self.peers.write().remove(&peer_index) {
			warn!(target: "sync", "Disconnecting from peer#{} due to misbehaving: {}", peer_index, reason);
			peer.connection.close();
		}
	}

	fn dos(&self, peer_index: PeerIndex, reason: &str) {
		if let Some(peer) = self.peers.write().remove(&peer_index) {
			warn!(target: "sync", "Disconnecting from peer#{} due to DOS: {}", peer_index, reason);
			peer.connection.close();
		}
	}
}

impl PeersFilters for PeersImpl {
	fn set_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterLoad) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.filter.load(filter);
		}
	}

	fn update_bloom_filter(&self, peer_index: PeerIndex, filter: types::FilterAdd) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.filter.add(filter);
		}
	}

	fn clear_bloom_filter(&self, peer_index: PeerIndex) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.filter.clear();
		}
	}

	fn set_fee_filter(&self, peer_index: PeerIndex, filter: types::FeeFilter) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.filter.set_fee_rate(filter);
		}
	}

	fn filter_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> BlockAnnouncementType {
		if let Some(peer) = self.peers.read().get(&peer_index) {
			if peer.filter.filter_block(&block.header.hash) {
				return peer.block_announcement_type
			}
		}

		BlockAnnouncementType::DoNotAnnounce
	}

	fn filter_transaction(&self, peer_index: PeerIndex, transaction: &IndexedTransaction, transaction_fee_rate: Option<u64>) -> TransactionAnnouncementType {
		if let Some(peer) = self.peers.read().get(&peer_index) {
			if peer.filter.filter_transaction(transaction, transaction_fee_rate) {
				return peer.transaction_announcement_type
			}
		}

		TransactionAnnouncementType::DoNotAnnounce
	}

	fn hash_known_as(&self, peer_index: PeerIndex, hash: H256, hash_type: KnownHashType) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.filter.hash_known_as(hash, hash_type)
		}
	}

	fn is_hash_known_as(&self, peer_index: PeerIndex, hash: &H256, hash_type: KnownHashType) -> bool {
		self.peers.read().get(&peer_index)
			.map(|peer| peer.filter.is_hash_known_as(hash, hash_type))
			.unwrap_or(false)
	}

	fn build_compact_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> Option<types::CompactBlock> {
		self.peers.read().get(&peer_index)
			.map(|peer| peer.filter.build_compact_block(block))
	}

	fn build_merkle_block(&self, peer_index: PeerIndex, block: &IndexedBlock) -> Option<MerkleBlockArtefacts> {
		self.peers.read().get(&peer_index)
			.and_then(|peer| peer.filter.build_merkle_block(block))
	}
}

impl PeersOptions for PeersImpl {
	fn set_block_announcement_type(&self, peer_index: PeerIndex, announcement_type: BlockAnnouncementType) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.block_announcement_type = announcement_type;
		}
	}

	fn set_transaction_announcement_type(&self, peer_index: PeerIndex, announcement_type: TransactionAnnouncementType) {
		if let Some(peer) = self.peers.write().get_mut(&peer_index) {
			peer.transaction_announcement_type = announcement_type;
		}
	}
}
