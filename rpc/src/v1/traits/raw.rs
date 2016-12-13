use jsonrpc_macros::Trailing;
use jsonrpc_core::Error;

use v1::types::H256;
use v1::types::RawTransaction;
use v1::types::Transaction;
use v1::types::TransactionInput;
use v1::types::TransactionOutput;
use v1::types::GetRawTransactionResponse;

build_rpc_trait! {
	/// Partiy-bitcoin raw data interface.
	pub trait Raw {
		/// Adds transaction to the memory pool && relays it to the peers.
		#[rpc(name = "sendrawtransaction")]
		fn send_raw_transaction(&self, RawTransaction) -> Result<H256, Error>;
		/// Create a transaction spending the given inputs and creating new outputs.
		#[rpc(name = "createrawtransaction")]
		fn create_raw_transaction(&self, Vec<TransactionInput>, Vec<TransactionOutput>, Trailing<u32>) -> Result<RawTransaction, Error>;
		/// Return an object representing the serialized, hex-encoded transaction.
		#[rpc(name = "decoderawtransaction")]
		fn decode_raw_transaction(&self, RawTransaction) -> Result<Transaction, Error>;
		/// Return the raw transaction data.
		#[rpc(name = "getrawtransaction")]
		fn get_raw_transaction(&self, H256, Trailing<bool>) -> Result<GetRawTransactionResponse, Error>;
	}
}
