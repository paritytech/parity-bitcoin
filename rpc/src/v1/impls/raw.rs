use v1::traits::Raw;
use v1::types::{RawTransaction, TransactionInput, TransactionOutputs, Transaction, GetRawTransactionResponse};
use v1::types::H256;
use v1::helpers::errors::{execution, invalid_params};
use jsonrpc_core::Error;
use jsonrpc_macros::Trailing;
use chain::Transaction as GlobalTransaction;
use sync;
use ser::{Reader, serialize, deserialize};
use primitives::bytes::Bytes as GlobalBytes;
use primitives::hash::H256 as GlobalH256;

pub struct RawClient<T: RawClientCoreApi> {
	core: T,
}

pub trait RawClientCoreApi: Send + Sync + 'static {
	fn accept_transaction(&self, transaction: GlobalTransaction) -> Result<GlobalH256, String>;
}

pub struct RawClientCore {
	local_sync_node: sync::LocalNodeRef,
}

impl RawClientCore {
	pub fn new(local_sync_node: sync::LocalNodeRef) -> Self {
		RawClientCore {
			local_sync_node: local_sync_node,
		}
	}
}

impl RawClientCoreApi for RawClientCore {
	fn accept_transaction(&self, transaction: GlobalTransaction) -> Result<GlobalH256, String> {
		self.local_sync_node.accept_transaction(transaction)
	}
}

impl<T> RawClient<T> where T: RawClientCoreApi {
	pub fn new(core: T) -> Self {
		RawClient {
			core: core,
		}
	}
}

impl<T> Raw for RawClient<T> where T: RawClientCoreApi {
	fn send_raw_transaction(&self, raw_transaction: RawTransaction) -> Result<H256, Error> {
		let raw_transaction_data: Vec<u8> = raw_transaction.into();
		let transaction = try!(deserialize(Reader::new(&raw_transaction_data)).map_err(|e| invalid_params("tx", e)));
		self.core.accept_transaction(transaction)
			.map(|h| h.reversed().into())
			.map_err(|e| execution(e))
	}

	fn create_raw_transaction(&self, inputs: Vec<TransactionInput>, outputs: TransactionOutputs, lock_time: Trailing<u32>) -> Result<RawTransaction, Error> {
		use chain;

		let lock_time = lock_time.unwrap_or(0);
		let is_locked = lock_time != 0;
		let inputs: Vec<_> = inputs.into_iter()
			.map(|input| chain::TransactionInput {
				previous_output: chain::OutPoint {
					hash: input.txid,
					index: input.vout,
				},
				script_sig: GlobalBytes::new(),
				sequence: if is_locked { chain::SEQUENCE_FINAL - 1 } else { chain::SEQUENCE_FINAL },
			});
		let outputs: Vec<_> = outputs.into_iter()
			.map(|output| chain::TransactionOutput {
				
			})
		let transaction = GlobalTransaction {
			version: 1,
			inputs: inuputs,
			outputs: outputs,
			lock_time: lock_time.unwrap_or(0),
		};
		Ok(serialize(transaction))
	}

	fn decode_raw_transaction(&self, _transaction: RawTransaction) -> Result<Transaction, Error> {
		rpc_unimplemented!()
	}

	fn get_raw_transaction(&self, _hash: H256, _verbose: Trailing<bool>) -> Result<GetRawTransactionResponse, Error> {
		rpc_unimplemented!()
	}
}

#[cfg(test)]
pub mod tests {
	use jsonrpc_core::{IoHandler, GenericIoHandler};
	use chain::Transaction;
	use primitives::hash::H256 as GlobalH256;
	use v1::traits::Raw;
	use super::*;

	#[derive(Default)]
	struct SuccessRawClientCore;
	#[derive(Default)]
	struct ErrorRawClientCore;

	impl RawClientCoreApi for SuccessRawClientCore {
		fn accept_transaction(&self, transaction: Transaction) -> Result<GlobalH256, String> {
			Ok(transaction.hash())
		}
	}

	impl RawClientCoreApi for ErrorRawClientCore {
		fn accept_transaction(&self, _transaction: Transaction) -> Result<GlobalH256, String> {
			Err("error".to_owned())
		}
	}

	#[test]
	fn sendrawtransaction_accepted() {
		let client = RawClient::new(SuccessRawClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "sendrawtransaction",
				"params": ["00000000013ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a0000000000000000000101000000000000000000000000"],
				"id": 1
			}"#)
		).unwrap();

		// direct hash is 0791efccd035c5fe501023ff888106eba5eff533965de4a6e06400f623bcac34
		// but client expects reverse hash
		assert_eq!(r#"{"jsonrpc":"2.0","result":"34acbc23f60064e0a6e45d9633f5efa5eb068188ff231050fec535d0ccef9107","id":1}"#, &sample);
	}

	#[test]
	fn sendrawtransaction_rejected() {
		let client = RawClient::new(ErrorRawClientCore::default());
		let handler = IoHandler::new();
		handler.add_delegate(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "sendrawtransaction",
				"params": ["00000000013ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a0000000000000000000101000000000000000000000000"],
				"id": 1
			}"#)
		).unwrap();

		assert_eq!(r#"{"jsonrpc":"2.0","error":{"code":-32015,"message":"Execution error.","data":"\"error\""},"id":1}"#, &sample);
	}
}
