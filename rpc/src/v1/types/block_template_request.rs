// TODO: remove after implementing getblocktmplate RPC
#![warn(dead_code)]

use std::collections::HashSet;

/// Block template request mode
pub enum BlockTemplateRequestMode {
	/// Work as described in BIP0022:
	/// https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki
	Template,
	/// Work as described in BIP0023:
	/// https://github.com/bitcoin/bips/blob/master/bip-0023.mediawiki
	Proposal,
}

/// Block template request parameters as described in:
/// https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0023.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0009.mediawiki#getblocktemplate_changes
/// https://github.com/bitcoin/bips/blob/master/bip-0145.mediawiki
pub struct BlockTemplateRequest {
	/// Request mode
	pub mode: Option<BlockTemplateRequestMode>,
	/// Capabilities, supported by client
	pub capabilities: Option<HashSet<String>>,
	/// Softfork deployments, supported by client
	pub rules: Option<HashSet<String>>,
}
