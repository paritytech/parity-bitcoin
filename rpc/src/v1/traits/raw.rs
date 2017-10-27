use jsonrpc_macros::Trailing;
use jsonrpc_core::Error;

use v1::types::H256;
use v1::types::RawTransaction;
use v1::types::Transaction;
use v1::types::TransactionInput;
use v1::types::TransactionOutputs;
use v1::types::GetRawTransactionResponse;

build_rpc_trait! {
	/// Parity-bitcoin raw data interface.
	pub trait Raw {
		/// Adds transaction to the memory pool && relays it to the peers.
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "sendrawtransaction", "params": ["01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"], "id":1 }' -H 'content-type: application/json' http://127.0.0.1:8332/
		#[rpc(name = "sendrawtransaction")]
		fn send_raw_transaction(&self, RawTransaction) -> Result<H256, Error>;
		/// Create a transaction spending the given inputs and creating new outputs.
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "createrawtransaction", "params": [[{"txid":"4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b","vout":0}],{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa":0.01}], "id":1 }' -H 'content-type: application/json' http://127.0.0.1:8332/
		#[rpc(name = "createrawtransaction")]
		fn create_raw_transaction(&self, Vec<TransactionInput>, TransactionOutputs, Trailing<u32>) -> Result<RawTransaction, Error>;
		/// Return an object representing the serialized, hex-encoded transaction.
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "decoderawtransaction", "params": ["01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"], "id":1 }' -H 'content-type: application/json' http://127.0.0.1:8332/
		#[rpc(name = "decoderawtransaction")]
		fn decode_raw_transaction(&self, RawTransaction) -> Result<Transaction, Error>;
		/// Return the raw transaction data.
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "getrawtransaction", "params": ["4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"], "id":1 }' -H 'content-type: application/json' http://127.0.0.1:8332/
		#[rpc(name = "getrawtransaction")]
		fn get_raw_transaction(&self, H256, Trailing<bool>) -> Result<GetRawTransactionResponse, Error>;
	}
}
