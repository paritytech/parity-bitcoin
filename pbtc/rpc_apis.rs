use std::str::FromStr;
use std::collections::HashSet;
use rpc::Dependencies;
use ethcore_rpc::MetaIoHandler;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum Api {
	/// Raw methods
	Raw,
	/// Miner-related methods
	Miner,
	/// BlockChain-related methods
	BlockChain,
	/// Network
	Network,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApiSet {
	List(HashSet<Api>),
}

impl Default for ApiSet {
	fn default() -> Self {
		ApiSet::List(vec![Api::Raw, Api::Miner, Api::BlockChain, Api::Network].into_iter().collect())
	}
}

impl FromStr for Api {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"raw" => Ok(Api::Raw),
			"miner" => Ok(Api::Miner),
			"blockchain" => Ok(Api::BlockChain),
			"network" => Ok(Api::Network),
			api => Err(format!("Unknown api: {}", api)),
		}
	}
}

impl ApiSet {
	pub fn list_apis(&self) -> HashSet<Api> {
		match *self {
			ApiSet::List(ref apis) => apis.clone(),
		}
	}
}

pub fn setup_rpc(mut handler: MetaIoHandler<()>, apis: ApiSet, deps: Dependencies) -> MetaIoHandler<()> {
	use ethcore_rpc::v1::*;

	for api in apis.list_apis() {
		match api {
			Api::Raw => handler.extend_with(RawClient::new(RawClientCore::new(deps.local_sync_node.clone())).to_delegate()),
			Api::Miner => handler.extend_with(MinerClient::new(MinerClientCore::new(deps.local_sync_node.clone())).to_delegate()),
			Api::BlockChain => handler.extend_with(BlockChainClient::new(BlockChainClientCore::new(deps.network, deps.storage.clone())).to_delegate()),
			Api::Network => handler.extend_with(NetworkClient::new(NetworkClientCore::new(deps.p2p_context.clone())).to_delegate()),
		}
	}

	handler
}
