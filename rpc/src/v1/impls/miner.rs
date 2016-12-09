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
}
