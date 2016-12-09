use std::collections::HashSet;

/// Block template request mode
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
pub enum BlockTemplateRequestMode {
	/// Work as described in BIP0022:
	/// https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki
	#[serde(rename="template")]
	Template,
	/// Work as described in BIP0023:
	/// https://github.com/bitcoin/bips/blob/master/bip-0023.mediawiki
	#[serde(rename="proposal")]
	Proposal,
}

/// Block template request parameters as described in:
/// https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0023.mediawiki
/// https://github.com/bitcoin/bips/blob/master/bip-0009.mediawiki#getblocktemplate_changes
/// https://github.com/bitcoin/bips/blob/master/bip-0145.mediawiki
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BlockTemplateRequest {
	/// Request mode
	pub mode: Option<BlockTemplateRequestMode>,
	/// Capabilities, supported by client
	pub capabilities: Option<HashSet<String>>,
	/// Softfork deployments, supported by client
	pub rules: Option<HashSet<String>>,
}

#[cfg(test)]
mod tests {
	use serde_json;
	use super::*;

	#[test]
	fn block_template_request_mode_serialize() {
		assert_eq!(serde_json::to_string(&BlockTemplateRequestMode::Template).unwrap(), r#""template""#);
		assert_eq!(serde_json::to_string(&BlockTemplateRequestMode::Proposal).unwrap(), r#""proposal""#);
	}

	#[test]
	fn block_template_request_mode_deserialize() {
		assert_eq!(serde_json::from_str::<BlockTemplateRequestMode>(r#""template""#).unwrap(), BlockTemplateRequestMode::Template);
		assert_eq!(serde_json::from_str::<BlockTemplateRequestMode>(r#""proposal""#).unwrap(), BlockTemplateRequestMode::Proposal);
	}

	#[test]
	fn block_template_request_serialize() {
		assert_eq!(serde_json::to_string(&BlockTemplateRequest::default()).unwrap(), r#"{"mode":null,"capabilities":null,"rules":null}"#);
		assert_eq!(serde_json::to_string(&BlockTemplateRequest {
			mode: Some(BlockTemplateRequestMode::Template),
			capabilities: Some(vec!["a".to_owned()].into_iter().collect()),
			rules: Some(vec!["b".to_owned()].into_iter().collect()),
		}).unwrap(), r#"{"mode":"template","capabilities":["a"],"rules":["b"]}"#);
	}

	#[test]
	fn block_template_request_deserialize() {
		assert_eq!(
			serde_json::from_str::<BlockTemplateRequest>(r#"{"mode":null,"capabilities":null,"rules":null}"#).unwrap(),
			BlockTemplateRequest {
				mode: None,
				capabilities: None,
				rules: None,
			});
		assert_eq!(
			serde_json::from_str::<BlockTemplateRequest>(r#"{"mode":"template","capabilities":["a"],"rules":["b"]}"#).unwrap(),
			BlockTemplateRequest {
				mode: Some(BlockTemplateRequestMode::Template),
				capabilities: Some(vec!["a".to_owned()].into_iter().collect()),
				rules: Some(vec!["b".to_owned()].into_iter().collect()),
			});
	}
}
