pub enum TransactionType {
	NonStandard,
	// Standard types
	PubKey,
	PubKeyHash,
	ScriptHash,
	Multisig,
	NullData,
	WitnessV0ScriptHash,
	WitnessV0KeyHash,
}
