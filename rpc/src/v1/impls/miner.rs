use v1::traits::Miner;
use v1::types::{BlockTemplate, BlockTemplateRequest};
use jsonrpc_core::Error;
use sync;
use miner;

pub struct MinerClient<T: MinerClientCoreApi> {
	core: T,
}

pub trait MinerClientCoreApi: Send + Sync + 'static {
	fn get_block_template(&self) -> miner::BlockTemplate;
}

pub struct MinerClientCore {
	local_sync_node: sync::LocalNodeRef,
}

impl MinerClientCore {
	pub fn new(local_sync_node: sync::LocalNodeRef) -> Self {
		MinerClientCore {
			local_sync_node: local_sync_node,
		}
	}
}

impl MinerClientCoreApi for MinerClientCore {
	fn get_block_template(&self) -> miner::BlockTemplate {
		self.local_sync_node.get_block_template()
	}
}

impl<T> MinerClient<T> where T: MinerClientCoreApi {
	pub fn new(core: T) -> Self {
		MinerClient {
			core: core,
		}
	}
}

impl<T> Miner for MinerClient<T> where T: MinerClientCoreApi {
	fn get_block_template(&self, _request: BlockTemplateRequest) -> Result<BlockTemplate, Error> {
		Ok(self.core.get_block_template().into())
	}
}

#[cfg(test)]
pub mod tests {
	use jsonrpc_core::IoHandler;
	use v1::traits::Miner;
	use primitives::hash::H256;
	use chain;
	use miner;
	use super::*;

	#[derive(Default)]
	struct SuccessMinerClientCore;

	impl MinerClientCoreApi for SuccessMinerClientCore {
		fn get_block_template(&self) -> miner::BlockTemplate {
			let tx: chain::Transaction = "00000000013ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a0000000000000000000101000000000000000000000000".into();
			miner::BlockTemplate {
				version: 777,
				previous_header_hash: H256::from(1),
				time: 33,
				bits: 44.into(),
				height: 55,
				transactions: vec![
					tx.into(),
				],
				coinbase_value: 66,
				size_limit: 77,
				sigop_limit: 88,
			}
		}
	}

	#[test]
	fn getblocktemplate_accepted() {
		let client = MinerClient::new(SuccessMinerClientCore::default());
		let mut handler = IoHandler::new();
		handler.extend_with(client.to_delegate());

		let sample = handler.handle_request_sync(&(r#"
			{
				"jsonrpc": "2.0",
				"method": "getblocktemplate",
				"params": [{}],
				"id": 1
			}"#)).unwrap();

		// direct hash is 0100000000000000000000000000000000000000000000000000000000000000
		// but client expects reverse hash
		assert_eq!(&sample, r#"{"jsonrpc":"2.0","result":{"bits":44,"coinbaseaux":null,"coinbasetxn":null,"coinbasevalue":66,"curtime":33,"height":55,"mintime":null,"mutable":null,"noncerange":null,"previousblockhash":"0000000000000000000000000000000000000000000000000000000000000001","rules":null,"sigoplimit":88,"sizelimit":77,"target":"0000000000000000000000000000000000000000000000000000000000000000","transactions":[{"data":"00000000013ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a0000000000000000000101000000000000000000000000","depends":null,"fee":null,"hash":null,"required":false,"sigops":null,"txid":null,"weight":null}],"vbavailable":null,"vbrequired":null,"version":777,"weightlimit":null},"id":1}"#);
	}
}
