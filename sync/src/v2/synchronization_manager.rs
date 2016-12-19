use primitives::hash::H256;
use synchronization_peers::Peers;
use utils::{OrphanBlocksPool, OrphanTransactionsPool};

struct ManagePeersConfig {
}

struct ManageUnknownBlocksConfig {
}

struct ManageOrphanTransactionsConfig {
}

pub fn manage_synchronization_peers_blocks(config: &ManagePeersConfig, peers: &mut Peers) -> (Vec<H256>, Vec<H256>) {
}

pub fn manage_synchronization_peers_inventory(config: &ManagePeersConfig, peers: &mut Peers) {
}

pub fn manage_unknown_orphaned_blocks(config: &ManageUnknownBlocksConfig, orphaned_blocks_pool: &mut OrphanBlocksPool) -> Option<Vec<H256>> {
}

pub fn manage_orphaned_transactions(config: &ManageOrphanTransactionsConfig, orphaned_transactions_pool: &mut OrphanTransactionsPool) -> Option<Vec<H256>> {
}
