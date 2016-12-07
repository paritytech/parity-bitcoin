use v1::traits::Raw;
use v1::types::RawTransaction;
use jsonrpc_core::Error;

pub struct RawClient;

impl Raw for RawClient {
	fn send_raw_transaction(&self, _tx: RawTransaction) -> Result<String, Error> {
		rpc_unimplemented!()
	}
}
