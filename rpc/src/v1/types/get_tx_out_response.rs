use super::hash::H256;
use super::transaction::TransactionOutputScript;

/// gettxout response
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GetTxOutResponse {
	/// Hash of the block this transaction output is included into.
	/// Why it's called 'best'? Who knows
	pub bestblock: H256,
	/// Number of confirmations of this transaction
	pub confirmations: u32,
	/// Transaction value in BTC
	pub value: f64,
	/// Script info
	#[serde(rename = "scriptPubKey")]
	pub script: TransactionOutputScript,
	/// This transaction version
	pub version: i32,
	/// Is this transactio a coinbase transaction?
	pub coinbase: bool,
}

#[cfg(test)]
mod tests {
	use serde_json;
	use super::super::bytes::Bytes;
	use super::super::hash::H256;
	use super::super::script::ScriptType;
	use super::super::transaction::TransactionOutputScript;
	use super::*;

	#[test]
	fn tx_out_response_serialize() {
		let txout = GetTxOutResponse {
			bestblock: H256::from(0x56),
			confirmations: 777,
			value: 100000.56,
			script: TransactionOutputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
				req_sigs: 777,
				script_type: ScriptType::Multisig,
				addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
			},
			version: 33,
			coinbase: false,
		};
		assert_eq!(serde_json::to_string(&txout).unwrap(), r#"{"bestblock":"5600000000000000000000000000000000000000000000000000000000000000","confirmations":777,"value":100000.56,"scriptPubKey":{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]},"version":33,"coinbase":false}"#);
	}

	#[test]
	fn tx_out_response_deserialize() {
		let txout = GetTxOutResponse {
			bestblock: H256::from(0x56),
			confirmations: 777,
			value: 100000.56,
			script: TransactionOutputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
				req_sigs: 777,
				script_type: ScriptType::Multisig,
				addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
			},
			version: 33,
			coinbase: false,
		};
		assert_eq!(
			serde_json::from_str::<GetTxOutResponse>(r#"{"bestblock":"5600000000000000000000000000000000000000000000000000000000000000","confirmations":777,"value":100000.56,"scriptPubKey":{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]},"version":33,"coinbase":false}"#).unwrap(),
			txout);
	}
}
