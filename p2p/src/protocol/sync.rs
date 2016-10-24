use std::sync::Arc;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{Command, Error, Payload, types, deserialize_payload};
use protocol::{Protocol, Direction};
use p2p::Context;
use PeerId;

pub type InboundSyncConnectionRef = Arc<Mutex<Box<InboundSyncConnection>>>;
pub type OutboundSyncConnectionRef = Arc<Mutex<Box<OutboundSyncConnection>>>;
pub type LocalSyncNodeRef = Arc<Mutex<Box<LocalSyncNode>>>;

// TODO: use this to respond to construct Version message (start_height field)
pub trait LocalSyncNode : Send + Sync {
	fn start_height(&self) -> i32;
	fn create_sync_session(&mut self, height: i32, outbound: OutboundSyncConnectionRef) -> InboundSyncConnectionRef;
}

pub trait InboundSyncConnection : Send + Sync {
	fn start_sync_session(&mut self, version: u32);
	fn on_inventory(&mut self, message: types::Inv);
	fn on_getdata(&mut self, message: types::GetData);
	fn on_getblocks(&mut self, message: types::GetBlocks);
	fn on_getheaders(&mut self, message: types::GetHeaders);
	fn on_transaction(&mut self, message: types::Tx);
	fn on_block(&mut self, message: types::Block);
	fn on_headers(&mut self, message: types::Headers);
	fn on_mempool(&mut self, message: types::MemPool);
	fn on_filterload(&mut self, message: types::FilterLoad);
	fn on_filteradd(&mut self, message: types::FilterAdd);
	fn on_filterclear(&mut self, message: types::FilterClear);
	fn on_merkleblock(&mut self, message: types::MerkleBlock);
	fn on_sendheaders(&mut self, message: types::SendHeaders);
	fn on_feefilter(&mut self, message: types::FeeFilter);
	fn on_send_compact(&mut self, message: types::SendCompact);
	fn on_compact_block(&mut self, message: types::CompactBlock);
	fn on_get_block_txn(&mut self, message: types::GetBlockTxn);
	fn on_block_txn(&mut self, message: types::BlockTxn);
}

pub trait OutboundSyncConnection : Send + Sync {
	fn send_inventory(&mut self, message: &types::Inv);
	fn send_getdata(&mut self, message: &types::GetData);
	fn send_getblocks(&mut self, message: &types::GetBlocks);
	fn send_getheaders(&mut self, message: &types::GetHeaders);
	fn send_transaction(&mut self, message: &types::Tx);
	fn send_block(&mut self, message: &types::Block);
	fn send_headers(&mut self, message: &types::Headers);
	fn send_mempool(&mut self, message: &types::MemPool);
	fn send_filterload(&mut self, message: &types::FilterLoad);
	fn send_filteradd(&mut self, message: &types::FilterAdd);
	fn send_filterclear(&mut self, message: &types::FilterClear);
	fn send_merkleblock(&mut self, message: &types::MerkleBlock);
	fn send_sendheaders(&mut self, message: &types::SendHeaders);
	fn send_feefilter(&mut self, message: &types::FeeFilter);
	fn send_send_compact(&mut self, message: &types::SendCompact);
	fn send_compact_block(&mut self, message: &types::CompactBlock);
	fn send_get_block_txn(&mut self, message: &types::GetBlockTxn);
	fn send_block_txn(&mut self, message: &types::BlockTxn);
}

struct OutboundSync {
	context: Arc<Context>,
	peer: PeerId,
}

impl OutboundSync {
	pub fn new(context: Arc<Context>, peer: PeerId) -> OutboundSync {
		OutboundSync {
			context: context,
			peer: peer,
		}
	}

	pub fn send_message<T>(&self, message: &T) where T: Payload {
		let send = Context::send_to_peer(self.context.clone(), self.peer, message);
		self.context.spawn(send);
	}

	pub fn boxed(self) -> Box<OutboundSyncConnection> {
		Box::new(self)
	}
}

impl OutboundSyncConnection for OutboundSync {
	fn send_inventory(&mut self, message: &types::Inv) {
		self.send_message(message);
	}

	fn send_getdata(&mut self, message: &types::GetData) {
		self.send_message(message);
	}

	fn send_getblocks(&mut self, message: &types::GetBlocks) {
		self.send_message(message);
	}

	fn send_getheaders(&mut self, message: &types::GetHeaders) {
		self.send_message(message);
	}

	fn send_transaction(&mut self, message: &types::Tx) {
		self.send_message(message);
	}

	fn send_block(&mut self, message: &types::Block) {
		self.send_message(message);
	}

	fn send_headers(&mut self, message: &types::Headers) {
		self.send_message(message);
	}

	fn send_mempool(&mut self, message: &types::MemPool) {
		self.send_message(message);
	}

	fn send_filterload(&mut self, message: &types::FilterLoad) {
		self.send_message(message);
	}

	fn send_filteradd(&mut self, message: &types::FilterAdd) {
		self.send_message(message);
	}

	fn send_filterclear(&mut self, message: &types::FilterClear) {
		self.send_message(message);
	}

	fn send_merkleblock(&mut self, message: &types::MerkleBlock) {
		self.send_message(message);
	}

	fn send_sendheaders(&mut self, message: &types::SendHeaders) {
		self.send_message(message);
	}

	fn send_feefilter(&mut self, message: &types::FeeFilter) {
		self.send_message(message);
	}

	fn send_send_compact(&mut self, message: &types::SendCompact) {
		self.send_message(message);
	}

	fn send_compact_block(&mut self, message: &types::CompactBlock) {
		self.send_message(message);
	}

	fn send_get_block_txn(&mut self, message: &types::GetBlockTxn) {
		self.send_message(message);
	}

	fn send_block_txn(&mut self, message: &types::BlockTxn) {
		self.send_message(message);
	}
}

pub struct SyncProtocol {
	inbound_connection: InboundSyncConnectionRef,
}

impl SyncProtocol {
	pub fn new(context: Arc<Context>, peer: PeerId) -> Self {
		let outbound_connection = Arc::new(Mutex::new(OutboundSync::new(context.clone(), peer).boxed()));
		let inbound_connection = context.create_sync_session(0, outbound_connection);
		SyncProtocol {
			inbound_connection: inbound_connection,
		}
	}
}

impl Protocol for SyncProtocol {
	fn initialize(&mut self, _direction: Direction, version: u32) -> Result<(), Error> {
		self.inbound_connection.lock().start_sync_session(version);
		Ok(())
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error> {
		if command == &types::Inv::command() {
			let message: types::Inv = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_inventory(message);
		}
		else if command == &types::GetData::command() {
			let message: types::GetData = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_getdata(message);
		}
		else if command == &types::GetBlocks::command() {
			let message: types::GetBlocks = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_getblocks(message);
		}
		else if command == &types::GetHeaders::command() {
			let message: types::GetHeaders = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_getheaders(message);
		}
		else if command == &types::Tx::command() {
			let message: types::Tx = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_transaction(message);
		}
		else if command == &types::Block::command() {
			let message: types::Block = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_block(message);
		}
		else if command == &types::MemPool::command() {
			let message: types::MemPool = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_mempool(message);
		}
		else if command == &types::Headers::command() {
			let message: types::Headers = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_headers(message);
		}
		else if command == &types::FilterLoad::command() {
			let message: types::FilterLoad = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_filterload(message);
		}
		else if command == &types::FilterAdd::command() {
			let message: types::FilterAdd = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_filteradd(message);
		}
		else if command == &types::FilterClear::command() {
			let message: types::FilterClear = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_filterclear(message);
		}
		else if command == &types::MerkleBlock::command() {
			let message: types::MerkleBlock = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_merkleblock(message);
		}
		else if command == &types::SendHeaders::command() {
			let message: types::SendHeaders = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_sendheaders(message);
		}
		else if command == &types::FeeFilter::command() {
			let message: types::FeeFilter = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_feefilter(message);
		}
		else if command == &types::SendCompact::command() {
			let message: types::SendCompact = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_send_compact(message);
		}
		else if command == &types::CompactBlock::command() {
			let message: types::CompactBlock = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_compact_block(message);
		}
		else if command == &types::GetBlockTxn::command() {
			let message: types::GetBlockTxn = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_get_block_txn(message);
		}
		else if command == &types::BlockTxn::command() {
			let message: types::BlockTxn = try!(deserialize_payload(payload, version));
			self.inbound_connection.lock().on_block_txn(message);
		}
		Ok(())
	}
}
