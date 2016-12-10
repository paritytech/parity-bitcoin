use super::bytes::Bytes;
use super::hash::{H160, H256};
use super::script::ScriptType;

/// gettxout response
#[derive(Debug, Serialize, Deserialize)]
pub struct GetTxOutResponse {
	/// Hash of the block this transaction output is included into.
	bestblock: H256,
	/// Number of confirmations of this transaction
	confirmations: u32,
	/// Transaction value in BTC
	value: f64,
	/// Script info
	#[serde(rename = "scriptPubKey")]
	script_pub_key: TxOutScriptPubKey,
	/// This transaction version
	version: i32,
	/// Is this transactio a coinbase transaction?
	coinbase: bool,
}

/// Script pub key information
#[derive(Debug, Serialize, Deserialize)]
pub struct TxOutScriptPubKey {
	/// Script code
	asm: String,
	/// Script hex
	hex: Bytes,
	/// Number of required signatures
	#[serde(rename = "reqSigs")]
	req_sigs: u32,
	/// Type of script
	#[serde(rename = "type")]
	script_type: ScriptType,
	/// Array of bitcoin addresses
	addresses: Vec<H160>,
}
