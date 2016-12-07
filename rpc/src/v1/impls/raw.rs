use v1::traits::Raw;
use v1::types::RawTransaction;
use v1::types::H256;
use v1::helpers::errors::execution;
use jsonrpc_core::Error;
use chain::Transaction;
use sync;
use ser::{Reader, deserialize};
use super::super::helpers::errors::invalid_params;

pub struct RawClient {
	local_sync_node: sync::LocalNodeRef,
}

impl RawClient {
	pub fn new(local_sync_node: sync::LocalNodeRef) -> Self {
		RawClient {
			local_sync_node: local_sync_node,
		}
	}
}

impl Raw for RawClient {
	fn send_raw_transaction(&self, raw_transaction: RawTransaction) -> Result<H256, Error> {
		let transaction: Transaction = try!(deserialize(Reader::new(&raw_transaction.0)).map_err(|err| invalid_params("tx", err)));
		match self.local_sync_node.accept_transaction(transaction) {
			// client expects to get reversed H256
			Ok(hash) => Ok(hash.reversed().into()),
			Err(err) => Err(execution(err)),
		}
	}
}
