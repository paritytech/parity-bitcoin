//! Block builder

use chain::{self, RepresentH256};
use primitives::hash::H256;
use primitives::bytes::Bytes;
use invoke::{Invoke, Identity};

pub struct BlockHashBuilder<F=Identity> {
	callback: F,
	block: Option<chain::Block>,
}

impl BlockHashBuilder {
	pub fn new() -> Self {
		BlockHashBuilder::with_callback(Identity)
	}
}

impl<F> BlockHashBuilder<F> where F: Invoke<(H256, chain::Block)> {
	pub fn with_callback(callback: F) -> Self {
		BlockHashBuilder {
			block: None,
			callback: callback,
		}
	}

	pub fn block(self) -> BlockBuilder<Self> {
		BlockBuilder::with_callback(self)
	}

	pub fn with_block(mut self, block: chain::Block) -> Self {
		self.block = Some(block);
		self
	}

	pub fn build(self) -> F::Result {
		let block = self.block.expect("Block is supposed to be build here to get hash");
		self.callback.invoke((
			block.hash(),
			block
		))
	}
}

impl<F> Invoke<chain::Block> for BlockHashBuilder<F>
	where F: Invoke<(H256, chain::Block)>
{
	type Result = Self;

	fn invoke(self, block: chain::Block) -> Self {
		self.with_block(block)
	}
}

pub struct BlockBuilder<F=Identity> {
	callback: F,
	header: Option<chain::BlockHeader>,
	transactions: Vec<chain::Transaction>,
}

impl BlockBuilder {
	pub fn new() -> Self {
		BlockBuilder::with_callback(Identity)
	}
}

impl<F> BlockBuilder<F> where F: Invoke<chain::Block> {
	pub fn with_callback(callback: F) -> Self {
		BlockBuilder {
			callback: callback,
			header: None,
			transactions: Vec::new(),
		}
	}

	pub fn with_header(mut self, header: chain::BlockHeader) -> Self {
		self.header = Some(header);
		self
	}

	pub fn with_transaction(mut self, transaction: chain::Transaction) -> Self {
		self.transactions.push(transaction);
		self
	}

	pub fn with_transactions<I>(mut self, txs: I) -> Self
		where I: IntoIterator<Item=chain::Transaction>
	{
		self.transactions.extend(txs);
		self
	}

	pub fn with_raw(mut self, raw: &'static str) -> Self {
		let raw_block: chain::Block = raw.into();
		self.transactions = raw_block.transactions.to_vec();
		self.header = Some(raw_block.header().clone());
		self
	}

	pub fn header(self) -> BlockHeaderBuilder<Self> {
		BlockHeaderBuilder::with_callback(self)
	}

	pub fn merkled_header(self) -> BlockHeaderBuilder<Self> {
		let hashes: Vec<H256> = self.transactions.iter().map(|t| t.hash()).collect();
		let builder = self.header().merkle_root(chain::merkle_root(&hashes));
		builder
	}

	pub fn transaction(self) -> TransactionBuilder<Self> {
		TransactionBuilder::with_callback(self)
	}

	pub fn derived_transaction(self, tx_idx: usize, output_idx: u32) -> TransactionBuilder<Self> {
		let tx = self.transactions.get(tx_idx).expect(&format!("using derive_transaction with the wrong index ({})", tx_idx)).clone();
		TransactionBuilder::with_callback(self).input().hash(tx.hash()).index(output_idx).build()
	}

	// use vec![(0, 1), (0, 2), (1, 1)]
	pub fn derived_transactions<I>(self, outputs: I) -> TransactionBuilder<Self>
		where I: IntoIterator<Item=(usize, u32)>
	{
		let mut derives = Vec::new();
		for (tx_idx, output_idx) in outputs {
			derives.push(
				(
					self.transactions.get(tx_idx).expect(&format!("using derive_transaction with the wrong index ({})", tx_idx)).hash(),
					output_idx
				)
			);
		}

		let mut builder = TransactionBuilder::with_callback(self);
		for (tx_hash, output_idx) in derives {
			builder = builder.input().hash(tx_hash).index(output_idx).build();
		}
		builder
	}

	pub fn build(self) -> F::Result {
		self.callback.invoke(
			chain::Block::new(
				self.header.unwrap(),
				self.transactions,
			)
		)
	}
}

impl<F> Invoke<chain::BlockHeader> for BlockBuilder<F>
	where F: Invoke<chain::Block>
{
	type Result = Self;

	fn invoke(self, header: chain::BlockHeader) -> Self {
		self.with_header(header)
	}
}

impl<F> Invoke<chain::Transaction> for BlockBuilder<F>
	where F: Invoke<chain::Block>
{
	type Result = Self;

	fn invoke(self, tx: chain::Transaction) -> Self {
		self.with_transaction(tx)
	}
}

pub struct BlockHeaderBuilder<F=Identity> {
	callback: F,
	time: u32,
	parent: H256,
	nonce: u32,
	nbits: u32,
	version: u32,
	merkle_root: H256,
}

impl<F> BlockHeaderBuilder<F> where F: Invoke<chain::BlockHeader> {
	pub fn with_callback(callback: F) -> Self {
		BlockHeaderBuilder {
			callback: callback,
			time: 0,
			nonce: 0,
			merkle_root: H256::from(0),
			parent: H256::from(0),
			nbits: 0,
			version: 1,
		}
	}

	pub fn parent(mut self, parent: H256) -> Self {
		self.parent = parent;
		self
	}

	pub fn time(mut self, time: u32) -> Self {
		self.time = time;
		self
	}

	pub fn merkle_root(mut self, merkle_root: H256) -> Self {
		self.merkle_root = merkle_root;
		self
	}

	pub fn nbits(mut self, nbits: u32) -> Self {
		self.nbits = nbits;
		self
	}

	pub fn nonce(mut self, nonce: u32) -> Self {
		self.nonce = nonce;
		self
	}

	pub fn build(self) -> F::Result {
		self.callback.invoke(
			chain::BlockHeader {
				time: self.time,
				previous_header_hash: self.parent,
				nbits: self.nbits,
				nonce: self.nonce,
				merkle_root_hash: self.merkle_root,
				version: self.version,
			}
		)
	}
}

pub struct TransactionBuilder<F=Identity> {
	callback: F,
	version: i32,
	lock_time: u32,
	inputs: Vec<chain::TransactionInput>,
	outputs: Vec<chain::TransactionOutput>,
}

impl<F> TransactionBuilder<F> where F: Invoke<chain::Transaction> {
	fn with_callback(callback: F) -> Self {
		TransactionBuilder {
			callback: callback,
			version: 1,
			lock_time: 0,
			inputs: Vec::new(),
			outputs: Vec::new(),
		}
	}

	fn with_input(mut self, input: chain::TransactionInput) -> Self {
		self.inputs.push(input);
		self
	}

	fn with_output(mut self, input: chain::TransactionOutput) -> Self {
		self.outputs.push(input);
		self
	}

	pub fn lock_time(mut self, time: u32) -> Self {
		self.lock_time = time;
		self
	}

	pub fn input(self) -> TransactionInputBuilder<Self> {
		TransactionInputBuilder::with_callback(self)
	}

	pub fn coinbase(self) -> Self {
		self.input().coinbase().build()
	}

	pub fn output(self) -> TransactionOutputBuilder<Self> {
		TransactionOutputBuilder::with_callback(self)
	}

	pub fn build(self) -> F::Result {
		self.callback.invoke(
			chain::Transaction {
				lock_time: self.lock_time,
				version: self.version,
				inputs: self.inputs,
				outputs: self.outputs,
			}
		)
	}
}


impl<F> Invoke<chain::TransactionInput> for TransactionBuilder<F>
	where F: Invoke<chain::Transaction>
{
	type Result = Self;

	fn invoke(self, tx: chain::TransactionInput) -> Self {
		self.with_input(tx)
	}
}

impl<F> Invoke<chain::TransactionOutput> for TransactionBuilder<F>
	where F: Invoke<chain::Transaction>
{
	type Result = Self;

	fn invoke(self, tx: chain::TransactionOutput) -> Self {
		self.with_output(tx)
	}
}

pub struct TransactionInputBuilder<F=Identity> {
	callback: F,
	output: Option<chain::OutPoint>,
	signature: Bytes,
	sequence: u32,
}

impl<F> TransactionInputBuilder<F> where F: Invoke<chain::TransactionInput> {
	fn with_callback(callback: F) -> Self {
		TransactionInputBuilder {
			callback: callback,
			output: None,
			signature: Bytes::new_with_len(0),
			sequence: 0,
		}
	}

	pub fn hash(mut self, hash: H256) -> Self {
		let mut output = self.output.unwrap_or(chain::OutPoint { hash: hash.clone(), index: 0 });
		output.hash = hash;
		self.output = Some(output);
		self
	}

	pub fn index(mut self, index: u32) -> Self {
		let mut output = self.output.unwrap_or(chain::OutPoint { hash: H256::from(0), index: index });
		output.index = index;
		self.output = Some(output);
		self
	}

	pub fn coinbase(mut self) -> Self {
		self.output = Some(chain::OutPoint { hash: H256::from(0), index: 0xffffffff });
		self
	}

	pub fn build(self) -> F::Result {
		self.callback.invoke(
			chain::TransactionInput {
				previous_output: self.output.unwrap_or_else(|| panic!("Building input without previous output")),
				script_sig: self.signature,
				sequence: self.sequence,
			}
		)
	}
}


pub struct TransactionOutputBuilder<F=Identity> {
	callback: F,
	value: u64,
	signature: Bytes,
}

impl<F> TransactionOutputBuilder<F> where F: Invoke<chain::TransactionOutput> {
	fn with_callback(callback: F) -> Self {
		TransactionOutputBuilder {
			callback: callback,
			signature: Bytes::new_with_len(0),
			value: 0,
		}
	}

	pub fn value(mut self, value: u64) -> Self {
		self.value = value;
		self
	}

	pub fn build(self) -> F::Result {
		self.callback.invoke(
			chain::TransactionOutput {
				script_pubkey: self.signature,
				value: self.value,
			}
		)
	}
}

pub fn block_builder() -> BlockBuilder { BlockBuilder::new() }
pub fn block_hash_builder() -> BlockHashBuilder { BlockHashBuilder::new() }

#[test]
fn example1() {
	let block = BlockBuilder::new().header().time(1000).build().build();
	assert_eq!(block.header().time, 1000);
}

#[test]
fn example2() {
	let block = BlockBuilder::new()
		.header().build()
		.transaction().lock_time(100500).build()
		.build();

	assert_eq!(block.transactions().len(), 1);
}

#[test]
fn example3() {
	let block = block_builder().header().build()
		.transaction().coinbase().build()
		.build();

	assert!(block.transactions()[0].is_coinbase());
}

#[test]
fn example4() {
	let block = block_builder().header().build()
		.transaction().coinbase()
			.output().value(10).build()
			.build()
		.transaction()
			.input().hash(H256::from(1)).index(1).build()
			.build()
		.build();

	assert_eq!(block.transactions().len(), 2);
	assert_eq!(block.transactions()[1].inputs[0].previous_output.hash, H256::from(1));
}

#[test]
fn example5() {
	let (hash, block) = block_hash_builder()
		.block()
			.header().parent(H256::from(0)).build()
			.build()
		.build();

	assert_eq!(hash, "9f54dbfe94217c473e9acd5f52303d85ce1ef5e563a7e55b378ad555089fdd4d".into());
	assert_eq!(block.header().previous_header_hash, "0000000000000000000000000000000000000000000000000000000000000000".into());
}
