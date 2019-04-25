use std::fmt;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::ser::SerializeMap;
use keys::Address;
use v1::types;
use super::bytes::Bytes;
use super::hash::H256;
use super::script::ScriptType;

/// Hex-encoded transaction
pub type RawTransaction = Bytes;

/// Transaction input
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionInput {
	/// Previous transaction id
	pub txid: H256,
	/// Previous transaction output index
	pub vout: u32,
	/// Sequence number
	pub sequence: Option<u32>,
}

/// Transaction output of form "address": amount
#[derive(Debug, PartialEq)]
pub struct TransactionOutputWithAddress {
	/// Receiver' address
	pub address: Address,
	/// Amount in BTC
	pub amount: f64,
}

/// Trasaction output of form "data": serialized(output script data)
#[derive(Debug, PartialEq)]
pub struct TransactionOutputWithScriptData {
	/// Serialized script data
	pub script_data: Bytes,
}

/// Transaction output
#[derive(Debug, PartialEq)]
pub enum TransactionOutput {
	/// Of form address: amount
	Address(TransactionOutputWithAddress),
	/// Of form data: script_data_bytes
	ScriptData(TransactionOutputWithScriptData),
}

/// Transaction outputs, which serializes/deserializes as KV-map
#[derive(Debug, PartialEq)]
pub struct TransactionOutputs {
	/// Transaction outputs
	pub outputs: Vec<TransactionOutput>,
}

/// Transaction input script
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionInputScript {
	/// Script code
	pub asm: String,
	/// Script hex
	pub hex: Bytes,
}

/// Transaction output script
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionOutputScript {
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
	#[serde(with = "types::address::vec")]
	pub addresses: Vec<Address>,
}

/// Signed transaction input
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedTransactionInput {
	/// Previous transaction id
	pub txid: H256,
	/// Previous transaction output index
	pub vout: u32,
	/// Input script
	pub script_sig: TransactionInputScript,
	/// Sequence number
	pub sequence: u32,
	/// Hex-encoded witness data (if any)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub txinwitness: Option<Vec<Bytes>>,
}

/// Signed transaction output
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedTransactionOutput {
	/// Output value in BTC
	pub value: f64,
	/// Output index
	pub n: u32,
	/// Output script
	#[serde(rename = "scriptPubKey")]
	pub script: TransactionOutputScript,
}

/// Transaction
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
	/// Raw transaction
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hex: Option<RawTransaction>,
	/// The transaction id (same as provided)
	pub txid: H256,
	/// The transaction hash (differs from txid for witness transactions)
	pub hash: H256,
	/// The serialized transaction size
	pub size: usize,
	/// The virtual transaction size (differs from size for witness transactions)
	pub vsize: usize,
	/// The version
	pub version: i32,
	/// The lock time
	pub locktime: i32,
	/// Transaction inputs
	pub vin: Vec<SignedTransactionInput>,
	/// Transaction outputs
	pub vout: Vec<SignedTransactionOutput>,
	/// Hash of the block this transaction is included in
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blockhash: Option<H256>,
	/// Number of confirmations of this transaction
	#[serde(skip_serializing_if = "Option::is_none")]
	pub confirmations: Option<u32>,
	/// The transaction time in seconds since epoch (Jan 1 1970 GMT)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub time: Option<u32>,
	/// The block time in seconds since epoch (Jan 1 1970 GMT)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blocktime: Option<u32>,
}

/// Return value of `getrawtransaction` method
#[derive(Debug, PartialEq)]
pub enum GetRawTransactionResponse {
	/// Return value when asking for raw transaction
	Raw(RawTransaction),
	/// Return value when asking for verbose transaction
	Verbose(Transaction),
}

impl Serialize for GetRawTransactionResponse {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
		match *self {
			GetRawTransactionResponse::Raw(ref raw_transaction) => raw_transaction.serialize(serializer),
			GetRawTransactionResponse::Verbose(ref verbose_transaction) => verbose_transaction.serialize(serializer),
		}
	}
}

impl TransactionOutputs {
	pub fn len(&self) -> usize {
		self.outputs.len()
	}
}

impl Serialize for TransactionOutputs {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
		let mut state = serializer.serialize_map(Some(self.len()))?;
		for output in &self.outputs {
			match output {
				&TransactionOutput::Address(ref address_output) => {
					state.serialize_entry(&address_output.address.to_string(), &address_output.amount)?;
				},
				&TransactionOutput::ScriptData(ref script_output) => {
					state.serialize_entry("data", &script_output.script_data)?;
				},
			}
		}
		state.end()
	}
}

impl<'a> Deserialize<'a> for TransactionOutputs {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'a> {
		use serde::de::{Visitor, MapAccess};

		struct TransactionOutputsVisitor;

		impl<'b> Visitor<'b> for TransactionOutputsVisitor {
			type Value = TransactionOutputs;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("a transaction output object")
			}

			fn visit_map<V>(self, mut visitor: V) -> Result<TransactionOutputs, V::Error> where V: MapAccess<'b> {
				let mut outputs: Vec<TransactionOutput> = Vec::with_capacity(visitor.size_hint().unwrap_or(0));

				while let Some(key) = try!(visitor.next_key::<String>()) {
					if &key == "data" {
						let value: Bytes = try!(visitor.next_value());
						outputs.push(TransactionOutput::ScriptData(TransactionOutputWithScriptData {
							script_data: value,
						}));
					} else {
						let address = types::address::AddressVisitor::default().visit_str(&key)?;
						let amount: f64 = try!(visitor.next_value());
						outputs.push(TransactionOutput::Address(TransactionOutputWithAddress {
							address: address,
							amount: amount,
						}));
					}
				}

				Ok(TransactionOutputs {
					outputs: outputs,
				})
			}
		}

		deserializer.deserialize_any(TransactionOutputsVisitor)
	}
}

#[cfg(test)]
mod tests {
	use serde_json;
	use super::super::bytes::Bytes;
	use super::super::hash::H256;
	use super::super::script::ScriptType;
	use super::*;

	#[test]
	fn transaction_input_serialize() {
		let txinput = TransactionInput {
			txid: H256::from(7),
			vout: 33,
			sequence: Some(88),
		};
		assert_eq!(serde_json::to_string(&txinput).unwrap(), r#"{"txid":"0700000000000000000000000000000000000000000000000000000000000000","vout":33,"sequence":88}"#);
	}

	#[test]
	fn transaction_input_deserialize() {
		let txinput = TransactionInput {
			txid: H256::from(7),
			vout: 33,
			sequence: Some(88),
		};

		assert_eq!(
			serde_json::from_str::<TransactionInput>(r#"{"txid":"0700000000000000000000000000000000000000000000000000000000000000","vout":33,"sequence":88}"#).unwrap(),
			txinput);
	}

	#[test]
	fn transaction_outputs_serialize() {
		let txout = TransactionOutputs {
			outputs: vec![
				TransactionOutput::Address(TransactionOutputWithAddress {
					address: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(),
					amount: 123.45,
				}),
				TransactionOutput::Address(TransactionOutputWithAddress {
					address: "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into(),
					amount: 67.89,
				}),
				TransactionOutput::ScriptData(TransactionOutputWithScriptData {
					script_data: Bytes::new(vec![1, 2, 3, 4]),
				}),
				TransactionOutput::ScriptData(TransactionOutputWithScriptData {
					script_data: Bytes::new(vec![5, 6, 7, 8]),
				}),
			]
		};
		assert_eq!(serde_json::to_string(&txout).unwrap(), r#"{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa":123.45,"1H5m1XzvHsjWX3wwU781ubctznEpNACrNC":67.89,"data":"01020304","data":"05060708"}"#);
	}

	#[test]
	fn transaction_outputs_deserialize() {
		let txout = TransactionOutputs {
			outputs: vec![
				TransactionOutput::Address(TransactionOutputWithAddress {
					address: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(),
					amount: 123.45,
				}),
				TransactionOutput::Address(TransactionOutputWithAddress {
					address: "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into(),
					amount: 67.89,
				}),
				TransactionOutput::ScriptData(TransactionOutputWithScriptData {
					script_data: Bytes::new(vec![1, 2, 3, 4]),
				}),
				TransactionOutput::ScriptData(TransactionOutputWithScriptData {
					script_data: Bytes::new(vec![5, 6, 7, 8]),
				}),
			]
		};
		assert_eq!(
			serde_json::from_str::<TransactionOutputs>(r#"{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa":123.45,"1H5m1XzvHsjWX3wwU781ubctznEpNACrNC":67.89,"data":"01020304","data":"05060708"}"#).unwrap(),
			txout);
	}

	#[test]
	fn transaction_input_script_serialize() {
		let txin = TransactionInputScript {
			asm: "Hello, world!!!".to_owned(),
			hex: Bytes::new(vec![1, 2, 3, 4]),
		};
		assert_eq!(serde_json::to_string(&txin).unwrap(), r#"{"asm":"Hello, world!!!","hex":"01020304"}"#);
	}

	#[test]
	fn transaction_input_script_deserialize() {
		let txin = TransactionInputScript {
			asm: "Hello, world!!!".to_owned(),
			hex: Bytes::new(vec![1, 2, 3, 4]),
		};
		assert_eq!(
			serde_json::from_str::<TransactionInputScript>(r#"{"asm":"Hello, world!!!","hex":"01020304"}"#).unwrap(),
			txin);
	}

	#[test]
	fn transaction_output_script_serialize() {
		let txout = TransactionOutputScript {
			asm: "Hello, world!!!".to_owned(),
			hex: Bytes::new(vec![1, 2, 3, 4]),
			req_sigs: 777,
			script_type: ScriptType::Multisig,
			addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
		};
		assert_eq!(serde_json::to_string(&txout).unwrap(), r#"{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]}"#);
	}

	#[test]
	fn transaction_output_script_deserialize() {
		let txout = TransactionOutputScript {
			asm: "Hello, world!!!".to_owned(),
			hex: Bytes::new(vec![1, 2, 3, 4]),
			req_sigs: 777,
			script_type: ScriptType::Multisig,
			addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
		};

		assert_eq!(
			serde_json::from_str::<TransactionOutputScript>(r#"{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]}"#).unwrap(),
			txout);
	}

	#[test]
	fn signed_transaction_input_serialize() {
		let txin = SignedTransactionInput {
			txid: H256::from(77),
			vout: 13,
			script_sig: TransactionInputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
			},
			sequence: 123,
			txinwitness: None,
		};
		assert_eq!(serde_json::to_string(&txin).unwrap(), r#"{"txid":"4d00000000000000000000000000000000000000000000000000000000000000","vout":13,"script_sig":{"asm":"Hello, world!!!","hex":"01020304"},"sequence":123}"#);
	}

	#[test]
	fn signed_transaction_input_deserialize() {
		let txin = SignedTransactionInput {
			txid: H256::from(77),
			vout: 13,
			script_sig: TransactionInputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
			},
			sequence: 123,
			txinwitness: None,
		};
		assert_eq!(
			serde_json::from_str::<SignedTransactionInput>(r#"{"txid":"4d00000000000000000000000000000000000000000000000000000000000000","vout":13,"script_sig":{"asm":"Hello, world!!!","hex":"01020304"},"sequence":123}"#).unwrap(),
			txin);
	}

	#[test]
	fn signed_transaction_output_serialize() {
		let txout = SignedTransactionOutput {
			value: 777.79,
			n: 12,
			script: TransactionOutputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
				req_sigs: 777,
				script_type: ScriptType::Multisig,
				addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
			},
		};
		assert_eq!(serde_json::to_string(&txout).unwrap(), r#"{"value":777.79,"n":12,"scriptPubKey":{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]}}"#);
	}

	#[test]
	fn signed_transaction_output_deserialize() {
		let txout = SignedTransactionOutput {
			value: 777.79,
			n: 12,
			script: TransactionOutputScript {
				asm: "Hello, world!!!".to_owned(),
				hex: Bytes::new(vec![1, 2, 3, 4]),
				req_sigs: 777,
				script_type: ScriptType::Multisig,
				addresses: vec!["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into(), "1H5m1XzvHsjWX3wwU781ubctznEpNACrNC".into()],
			},
		};
		assert_eq!(
			serde_json::from_str::<SignedTransactionOutput>(r#"{"value":777.79,"n":12,"scriptPubKey":{"asm":"Hello, world!!!","hex":"01020304","reqSigs":777,"type":"multisig","addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","1H5m1XzvHsjWX3wwU781ubctznEpNACrNC"]}}"#).unwrap(),
			txout);
	}

	#[test]
	fn transaction_serialize() {
		let tx = Transaction {
			hex: Some("DEADBEEF".into()),
			txid: H256::from(4),
			hash: H256::from(5),
			size: 33,
			vsize: 44,
			version: 55,
			locktime: 66,
			vin: vec![],
			vout: vec![],
			blockhash: Some(H256::from(6)),
			confirmations: Some(77),
			time: Some(88),
			blocktime: Some(99),
		};
		assert_eq!(serde_json::to_string(&tx).unwrap(), r#"{"hex":"deadbeef","txid":"0400000000000000000000000000000000000000000000000000000000000000","hash":"0500000000000000000000000000000000000000000000000000000000000000","size":33,"vsize":44,"version":55,"locktime":66,"vin":[],"vout":[],"blockhash":"0600000000000000000000000000000000000000000000000000000000000000","confirmations":77,"time":88,"blocktime":99}"#);
	}

	#[test]
	fn transaction_deserialize() {
		let tx = Transaction {
			hex: Some("DEADBEEF".into()),
			txid: H256::from(4),
			hash: H256::from(5),
			size: 33,
			vsize: 44,
			version: 55,
			locktime: 66,
			vin: vec![],
			vout: vec![],
			blockhash: Some(H256::from(6)),
			confirmations: Some(77),
			time: Some(88),
			blocktime: Some(99),
		};
		assert_eq!(
			serde_json::from_str::<Transaction>(r#"{"hex":"deadbeef","txid":"0400000000000000000000000000000000000000000000000000000000000000","hash":"0500000000000000000000000000000000000000000000000000000000000000","size":33,"vsize":44,"version":55,"locktime":66,"vin":[],"vout":[],"blockhash":"0600000000000000000000000000000000000000000000000000000000000000","confirmations":77,"time":88,"blocktime":99}"#).unwrap(),
			tx);
	}
}
