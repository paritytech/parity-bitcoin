#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::Arc;
use parking_lot::Mutex;
use chain::{Block, Transaction};
use message::types;
use message::Error;
use bytes::Bytes;
use message::common::Command;
use protocol::{Protocol, ProtocolAction};

pub type InboundSyncConnectionRef = Arc<Mutex<Box<InboundSyncConnection>>>;

pub type OutboundSyncConnectionRef = Arc<Mutex<Box<OutboundSyncConnection>>>;
// TODO: use this to respond to construct Version message (start_height field)
// TODO: use this to create new inbound sessions
pub trait LocalSyncNode : Send + Sync {
	fn start_height(&self) -> i32;
	fn start_sync_session(&mut self, outbound: OutboundSyncConnectionRef) -> InboundSyncConnectionRef;
}

pub trait InboundSyncConnection : Send + Sync {
	fn on_iventory(&mut self, message: &types::Inv);
	fn on_getdata(&mut self, message: &types::GetData);
	fn on_getblocks(&mut self, message: &types::GetBlocks);
	fn on_getheaders(&mut self, message: &types::GetHeaders);
	fn on_transaction(&mut self, message: &Transaction);
	fn on_block(&mut self, message: &Block);
	fn on_headers(&mut self, message: &types::Headers);
	fn on_mempool(&mut self, message: &types::MemPool);
	fn on_filterload(&mut self, message: &types::FilterLoad);
	fn on_filteradd(&mut self, message: &types::FilterAdd);
	fn on_filterclear(&mut self, message: &types::FilterClear);
	fn on_merkleblock(&mut self, message: &types::MerkleBlock);
	fn on_sendheaders(&mut self, message: &types::SendHeaders);
	fn on_feefilter(&mut self, message: &types::FeeFilter);
	fn on_send_compact(&mut self, message: &types::SendCompact);
	fn on_compact_block(&mut self, message: &types::CompactBlock);
	fn on_get_block_txn(&mut self, message: &types::GetBlockTxn);
	fn on_block_txn(&mut self, message: &types::BlockTxn);
}

pub trait OutboundSyncConnection : Send + Sync {
	fn send_iventory(&mut self, message: &types::Inv);
	fn send_getdata(&mut self, message: &types::GetData);
	fn send_getblocks(&mut self, message: &types::GetBlocks);
	fn send_getheaders(&mut self, message: &types::GetHeaders);
	fn send_transaction(&mut self, message: &Transaction);
	fn send_block(&mut self, message: &Block);
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
}

impl OutboundSync {
	pub fn new() -> OutboundSync {
		OutboundSync {
		}
	}
}

impl OutboundSyncConnection for OutboundSync {
	fn send_iventory(&mut self, message: &types::Inv) {
		unimplemented!()
	}

	fn send_getdata(&mut self, message: &types::GetData) {
		unimplemented!()
	}

	fn send_getblocks(&mut self, message: &types::GetBlocks) {
		unimplemented!()
	}

	fn send_getheaders(&mut self, message: &types::GetHeaders) {
		unimplemented!()
	}

	fn send_transaction(&mut self, message: &Transaction) {
		unimplemented!()
	}

	fn send_block(&mut self, message: &Block) {
		unimplemented!()
	}

	fn send_headers(&mut self, message: &types::Headers) {
		unimplemented!()
	}

	fn send_mempool(&mut self, message: &types::MemPool) {
		unimplemented!()
	}

	fn send_filterload(&mut self, message: &types::FilterLoad) {
		unimplemented!()
	}

	fn send_filteradd(&mut self, message: &types::FilterAdd) {
		unimplemented!()
	}

	fn send_filterclear(&mut self, message: &types::FilterClear) {
		unimplemented!()
	}

	fn send_merkleblock(&mut self, message: &types::MerkleBlock) {
		unimplemented!()
	}

	fn send_sendheaders(&mut self, message: &types::SendHeaders) {
		unimplemented!()
	}

	fn send_feefilter(&mut self, message: &types::FeeFilter) {
		unimplemented!()
	}

	fn send_send_compact(&mut self, message: &types::SendCompact) {
		unimplemented!()
	}

	fn send_compact_block(&mut self, message: &types::CompactBlock) {
		unimplemented!()
	}

	fn send_get_block_txn(&mut self, message: &types::GetBlockTxn) {
		unimplemented!()
	}

	fn send_block_txn(&mut self, message: &types::BlockTxn) {
		unimplemented!()
	}
}

pub struct SyncProtocol {
	//inbound_connection: InboundSyncConnectionRef,
	//outbound_connection: OutboundSyncConnectionRef,
}

impl SyncProtocol {
	// TODO: pass session/channel to allow sending messages at any time
	pub fn new() -> Self {
		// let outbound_connection = ... // TODO: create outbound connection for given session/channel
		// let inbound_connection = local_sync_node.start_sync_session(outbound_connection); // TODO: create inbound connection using LocalSyncNode::start_sync_session
		SyncProtocol {
		//	inbound_connection: inbound_connection,
		//	outbound_connection: OutboundSync::new(),
		}
	}
}

impl Protocol for SyncProtocol {
	fn on_message(&self, command: &Command, payload: &Bytes, version: u32) -> Result<ProtocolAction, Error> {
		// TODO: pass message to inbound_connection + convert response to ProtocolAction/Error
		/*
		if command == &Inv::command().into() {
			let inventory: Inv = try!(deserialize_payload(payload, version));
			self.inbound_connection.on_iventory(&inventory);
		} else {
			Ok(ProtocolAction::None)
		}
		*/
		unimplemented!()
	}
}