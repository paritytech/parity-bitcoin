use super::bytes::Bytes;
use super::hash::{H160, H256};
use super::script::ScriptType;

/// gettxout response
#[derive(Debug, Default, Serialize, Deserialize)]
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
	pub script_pub_key: TxOutScriptPubKey,
	/// This transaction version
	pub version: i32,
	/// Is this transactio a coinbase transaction?
	pub coinbase: bool,
}

/// Script pub key information
#[derive(Debug, Serialize, Deserialize)]
pub struct TxOutScriptPubKey {
	/// Script code
	pub asm: String,
	/// Script hex
	pub hex: Bytes,
	/// Number of required signatures
	#[serde(rename = "reqSigs")]
	pub req_sigs: u32,
	/// Type of script
	#[serde(rename = "type")]
	pub script_type: ScriptType,
	/// Array of bitcoin addresses
	pub addresses: Vec<H160>,
}

// TODO: remove me
impl Default for TxOutScriptPubKey {
	fn default() -> Self {
		TxOutScriptPubKey {
			asm: String::default(),
			hex: Bytes::default(),
			req_sigs: u32::default(),
			script_type: ScriptType::NonStandard,
			addresses: vec![],
		}
	}
}
