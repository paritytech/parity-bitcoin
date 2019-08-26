use std::sync::Arc;
use bytes::Bytes;
use message::{Command, Error, Payload, Services, types, deserialize_payload};
use protocol::Protocol;
use net::PeerContext;
use ser::SERIALIZE_TRANSACTION_WITNESS;

pub type InboundSyncConnectionRef = Box<dyn InboundSyncConnection>;
pub type OutboundSyncConnectionRef = Arc<dyn OutboundSyncConnection>;
pub type LocalSyncNodeRef = Box<dyn LocalSyncNode>;
pub type InboundSyncConnectionStateRef = Arc<dyn InboundSyncConnectionState>;

pub trait LocalSyncNode : Send + Sync {
	fn create_sync_session(&self, height: i32, services: Services, outbound: OutboundSyncConnectionRef) -> InboundSyncConnectionRef;
}

pub trait InboundSyncConnectionState: Send + Sync {
	fn synchronizing(&self) -> bool;
}

pub trait InboundSyncConnection : Send + Sync {
	fn sync_state(&self) -> InboundSyncConnectionStateRef;
	fn start_sync_session(&self, peer_name: String, version: types::Version);
	fn close_session(&self);
	fn on_inventory(&self, message: types::Inv);
	fn on_getdata(&self, message: types::GetData);
	fn on_getblocks(&self, message: types::GetBlocks);
	fn on_getheaders(&self, message: types::GetHeaders, id: u32);
	fn on_transaction(&self, message: types::Tx);
	fn on_block(&self, message: types::Block);
	fn on_headers(&self, message: types::Headers);
	fn on_mempool(&self, message: types::MemPool);
	fn on_filterload(&self, message: types::FilterLoad);
	fn on_filteradd(&self, message: types::FilterAdd);
	fn on_filterclear(&self, message: types::FilterClear);
	fn on_merkleblock(&self, message: types::MerkleBlock);
	fn on_sendheaders(&self, message: types::SendHeaders);
	fn on_feefilter(&self, message: types::FeeFilter);
	fn on_send_compact(&self, message: types::SendCompact);
	fn on_compact_block(&self, message: types::CompactBlock);
	fn on_get_block_txn(&self, message: types::GetBlockTxn);
	fn on_block_txn(&self, message: types::BlockTxn);
	fn on_notfound(&self, message: types::NotFound);
}

pub trait OutboundSyncConnection : Send + Sync {
	fn send_inventory(&self, message: &types::Inv);
	fn send_getdata(&self, message: &types::GetData);
	fn send_getblocks(&self, message: &types::GetBlocks);
	fn send_getheaders(&self, message: &types::GetHeaders);
	fn send_transaction(&self, message: &types::Tx);
	fn send_block(&self, message: &types::Block);
	fn send_witness_transaction(&self, message: &types::Tx);
	fn send_witness_block(&self, message: &types::Block);
	fn send_headers(&self, message: &types::Headers);
	fn respond_headers(&self, message: &types::Headers, id: u32);
	fn send_mempool(&self, message: &types::MemPool);
	fn send_filterload(&self, message: &types::FilterLoad);
	fn send_filteradd(&self, message: &types::FilterAdd);
	fn send_filterclear(&self, message: &types::FilterClear);
	fn send_merkleblock(&self, message: &types::MerkleBlock);
	fn send_sendheaders(&self, message: &types::SendHeaders);
	fn send_feefilter(&self, message: &types::FeeFilter);
	fn send_send_compact(&self, message: &types::SendCompact);
	fn send_compact_block(&self, message: &types::CompactBlock);
	fn send_get_block_txn(&self, message: &types::GetBlockTxn);
	fn send_block_txn(&self, message: &types::BlockTxn);
	fn send_notfound(&self, message: &types::NotFound);
	fn ignored(&self, id: u32);
	fn close(&self);
}

struct OutboundSync {
	context: Arc<PeerContext>,
}

impl OutboundSync {
	pub fn new(context: Arc<PeerContext>) -> OutboundSync {
		OutboundSync {
			context: context,
		}
	}
}

impl OutboundSyncConnection for OutboundSync {
	fn send_inventory(&self, message: &types::Inv) {
		self.context.send_request(message);
	}

	fn send_getdata(&self, message: &types::GetData) {
		self.context.send_request(message);
	}

	fn send_getblocks(&self, message: &types::GetBlocks) {
		self.context.send_request(message);
	}

	fn send_getheaders(&self, message: &types::GetHeaders) {
		self.context.send_request(message);
	}

	fn send_transaction(&self, message: &types::Tx) {
		self.context.send_request(message);
	}

	fn send_block(&self, message: &types::Block) {
		self.context.send_request(message);
	}

	fn send_witness_transaction(&self, message: &types::Tx) {
		self.context.send_request_with_flags(message, SERIALIZE_TRANSACTION_WITNESS);
	}

	fn send_witness_block(&self, message: &types::Block) {
		self.context.send_request_with_flags(message, SERIALIZE_TRANSACTION_WITNESS);
	}

	fn send_headers(&self, message: &types::Headers) {
		self.context.send_request(message);
	}

	fn respond_headers(&self, message: &types::Headers, id: u32) {
		self.context.send_response(message, id, true);
	}

	fn send_mempool(&self, message: &types::MemPool) {
		self.context.send_request(message);
	}

	fn send_filterload(&self, message: &types::FilterLoad) {
		self.context.send_request(message);
	}

	fn send_filteradd(&self, message: &types::FilterAdd) {
		self.context.send_request(message);
	}

	fn send_filterclear(&self, message: &types::FilterClear) {
		self.context.send_request(message);
	}

	fn send_merkleblock(&self, message: &types::MerkleBlock) {
		self.context.send_request(message);
	}

	fn send_sendheaders(&self, message: &types::SendHeaders) {
		self.context.send_request(message);
	}

	fn send_feefilter(&self, message: &types::FeeFilter) {
		self.context.send_request(message);
	}

	fn send_send_compact(&self, message: &types::SendCompact) {
		self.context.send_request(message);
	}

	fn send_compact_block(&self, message: &types::CompactBlock) {
		self.context.send_request(message);
	}

	fn send_get_block_txn(&self, message: &types::GetBlockTxn) {
		self.context.send_request(message);
	}

	fn send_block_txn(&self, message: &types::BlockTxn) {
		self.context.send_request(message);
	}

	fn send_notfound(&self, message: &types::NotFound) {
		self.context.send_request(message);
	}

	fn ignored(&self, id: u32) {
		self.context.ignore_response(id);
	}

	fn close(&self) {
		self.context.global().penalize_node(&self.context.info().address);
		self.context.close()
	}
}

pub struct SyncProtocol {
	inbound_connection: InboundSyncConnectionRef,
	context: Arc<PeerContext>,
	state: InboundSyncConnectionStateRef,
}

impl SyncProtocol {
	pub fn new(context: Arc<PeerContext>) -> Self {
		let outbound_connection = Arc::new(OutboundSync::new(context.clone()));
		let inbound_connection = context.global().create_sync_session(0, context.info().version_message.services(), outbound_connection);
		let state = inbound_connection.sync_state();
		SyncProtocol {
			inbound_connection: inbound_connection,
			context: context,
			state: state,
		}
	}
}

impl Protocol for SyncProtocol {
	fn initialize(&mut self) {
		let info = self.context.info();
		self.inbound_connection.start_sync_session(
			format!("{}/{}", info.address, info.user_agent),
			info.version_message.clone()
		);
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		let version = self.context.info().version;
		if command == &types::Inv::command() {
			// we are synchronizing => we ask only for blocks with known headers
			// => there are no useful blocks hashes for us
			// we are synchronizing
			// => we ignore all transactions until it is completed => there are no useful transactions hashes for us
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::Inv = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_inventory(message);
		}
		else if command == &types::GetData::command() {
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::GetData = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_getdata(message);
		}
		else if command == &types::GetBlocks::command() {
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::GetBlocks = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_getblocks(message);
		}
		else if command == &types::GetHeaders::command() {
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::GetHeaders = try!(deserialize_payload(payload, version));
			let id = self.context.declare_response();
			trace!("declared response {} for request: {}", id, types::GetHeaders::command());
			self.inbound_connection.on_getheaders(message, id);
		}
		else if command == &types::Tx::command() {
			// we ignore all transactions while synchronizing, as memory pool contains
			// only verified transactions && we can not verify on-top transactions while
			// we are not on the top
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::Tx = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_transaction(message);
		}
		else if command == &types::Block::command() {
			let message: types::Block = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_block(message);
		}
		else if command == &types::MemPool::command() {
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::MemPool = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_mempool(message);
		}
		else if command == &types::Headers::command() {
			let message: types::Headers = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_headers(message);
		}
		else if command == &types::FilterLoad::command() {
			let message: types::FilterLoad = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_filterload(message);
		}
		else if command == &types::FilterAdd::command() {
			let message: types::FilterAdd = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_filteradd(message);
		}
		else if command == &types::FilterClear::command() {
			let message: types::FilterClear = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_filterclear(message);
		}
		else if command == &types::MerkleBlock::command() {
			let message: types::MerkleBlock = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_merkleblock(message);
		}
		else if command == &types::SendHeaders::command() {
			let message: types::SendHeaders = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_sendheaders(message);
		}
		else if command == &types::FeeFilter::command() {
			let message: types::FeeFilter = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_feefilter(message);
		}
		else if command == &types::SendCompact::command() {
			let message: types::SendCompact = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_send_compact(message);
		}
		else if command == &types::CompactBlock::command() {
			let message: types::CompactBlock = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_compact_block(message);
		}
		else if command == &types::GetBlockTxn::command() {
			if self.state.synchronizing() {
				return Ok(());
			}

			let message: types::GetBlockTxn = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_get_block_txn(message);
		}
		else if command == &types::BlockTxn::command() {
			let message: types::BlockTxn = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_block_txn(message);
		}
		else if command == &types::NotFound::command() {
			let message: types::NotFound = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_notfound(message);
		}
		Ok(())
	}

	fn on_close(&mut self) {
		self.inbound_connection.close_session()
	}
}
