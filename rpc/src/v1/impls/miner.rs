use v1::traits::Miner;
use v1::types::{BlockTemplate, BlockTemplateRequest};
use jsonrpc_core::Error;

pub struct MinerClient<T: MinerClientCoreApi> {
	_core: T,
}

pub trait MinerClientCoreApi: Send + Sync + 'static {
}

pub struct MinerClientCore {
}

impl MinerClientCore {
	pub fn new() -> Self {
		MinerClientCore {
		}
	}
}

impl MinerClientCoreApi for MinerClientCore {
}

impl<T> MinerClient<T> where T: MinerClientCoreApi {
	pub fn new(core: T) -> Self {
		MinerClient {
			_core: core,
		}
	}
}

impl<T> Miner for MinerClient<T> where T: MinerClientCoreApi {
	fn get_block_template(&self, _request: BlockTemplateRequest) -> Result<BlockTemplate, Error> {
		rpc_unimplemented!()
	}
}

#[cfg(test)]
pub mod tests {
}
