use parking_lot::Mutex;
use bit_vec::BitVec;
use murmur3::murmur3_32;
use chain::{IndexedTransaction, OutPoint};
use message::types;
use ser::serialize;
use script::Script;

/// Constant optimized to create large differences in the seed for different values of `hash_functions_num`.
const SEED_OFFSET: u32 = 0xFBA4C795;

/// Connection bloom filter
#[derive(Debug)]
pub struct BloomFilter {
	/// Bloom data. Filter can be updated when transaction is matched => we have to use some kind of lock here.
	/// Mutex is an only choice, because:
	/// 1) we do not know if transaction matches the filter in advance
	/// 2) RwLock is non-upgradeable in Rust
	bloom: Option<Mutex<BloomFilterData>>,
	/// Filter update type.
	filter_flags: types::FilterFlags,
}

/// Bloom filter data implemented as described in:
/// https://github.com/bitcoin/bips/blob/master/bip-0037.mediawiki
#[derive(Debug, Default)]
struct BloomFilterData {
	/// Filter storage
	filter: BitVec,
	/// Number of hash functions to use in bloom filter
	hash_functions_num: u32,
	/// Value to add to Murmur3 hash seed when calculating hash
	tweak: u32,
}

impl Default for BloomFilter {
	fn default() -> Self {
		BloomFilter {
			bloom: None,
			filter_flags: types::FilterFlags::None,
		}
	}
}

impl BloomFilter {
	/// Create with given parameters
	#[cfg(test)]
	pub fn with_filter_load(message: types::FilterLoad) -> Self {
		BloomFilter {
			filter_flags: message.flags,
			bloom: Some(Mutex::new(BloomFilterData::with_filter_load(message))),
		}
	}

	/// Returns true if bloom filter is set
	pub fn is_set(&self) -> bool {
		self.bloom.is_some()
	}

	/// Sets bloom filter to given value
	pub fn set_bloom_filter(&mut self, message: types::FilterLoad) {
		self.bloom = Some(Mutex::new(BloomFilterData::with_filter_load(message)));
	}

	/// Adds given data to current filter, so that new transactions can be accepted
	pub fn update_bloom_filter(&mut self, message: types::FilterAdd) {
		if let Some(ref mut bloom) = self.bloom {
			bloom.lock().insert(&message.data);
		}
	}

	/// Removes bloom filter, so that all transactions are now accepted by this filter
	pub fn remove_bloom_filter(&mut self) {
		self.bloom = None;
	}

	/// Filters transaction using bloom filter data
	pub fn filter_transaction(&self, tx: &IndexedTransaction) -> bool {
		// check with bloom filter, if set
		match self.bloom {
			// if no filter is set for the connection => match everything
			None => true,
			// filter using bloom filter, then update
			Some(ref bloom) => {
				let mut bloom = bloom.lock();
				let mut is_match = false;

				// match if filter contains any arbitrary script data element in any scriptPubKey in tx
				for (output_index, output) in tx.raw.outputs.iter().enumerate() {
					let script = Script::new(output.script_pubkey.clone());
					let is_update_needed = self.filter_flags == types::FilterFlags::All
						|| (self.filter_flags == types::FilterFlags::PubKeyOnly && (script.is_pay_to_public_key() || script.is_multisig_script()));
					if contains_any_instruction_data(&*bloom, script) {
						is_match = true;

						if is_update_needed {
							bloom.insert(&serialize(&OutPoint {
								hash: tx.hash.clone(),
								index: output_index as u32,
							}));
						}
					}
				}

				// filter is updated only above => we can early-return from now
				if is_match {
					return is_match;
				}

				// match if filter contains transaction itself
				if bloom.contains(&*tx.hash) {
					return true;
				}

				// match if filter contains an outpoint this transaction spends
				for input in &tx.raw.inputs {
					// check if match previous output
					let previous_output = serialize(&input.previous_output);
					is_match = bloom.contains(&*previous_output);
					if is_match {
						return true;
					}

					// check if match any arbitrary script data element in any scriptSig in tx
					let script = Script::new(input.script_sig.clone());
					if contains_any_instruction_data(&*bloom, script) {
						return true;
					}
				}

				// no matches
				false
			},
		}
	}
}

impl BloomFilterData {
	/// Create with given parameters
	pub fn with_filter_load(message: types::FilterLoad) -> Self {
		BloomFilterData {
			filter: BitVec::from_bytes(&message.filter),
			hash_functions_num: message.hash_functions,
			tweak: message.tweak,
		}
	}

	/// True if filter contains given bytes
	pub fn contains(&self, data: &[u8]) -> bool {
		for hash_function_idx in 0..self.hash_functions_num {
			let murmur_seed = hash_function_idx.overflowing_mul(SEED_OFFSET).0.overflowing_add(self.tweak).0;
			let murmur_hash = murmur3_32(&mut data.as_ref(), murmur_seed) as usize % self.filter.len();
			let index = (murmur_hash & !7usize) | ((murmur_hash & 7) ^ 7);
			if !self.filter.get(index).expect("murmur_hash is result of mod operation by filter len; qed") {
				return false;
			}
		}
		true
	}

	/// Add bytes to the filter
	pub fn insert(&mut self, data: &[u8]) {
		for hash_function_idx in 0..self.hash_functions_num {
			let murmur_seed = hash_function_idx.overflowing_mul(SEED_OFFSET).0.overflowing_add(self.tweak).0;
			let murmur_hash = murmur3_32(&mut data.as_ref(), murmur_seed) as usize % self.filter.len();
			let index = (murmur_hash & !7usize) | ((murmur_hash & 7) ^ 7);
			self.filter.set(index, true);
		}
	}
}

fn contains_any_instruction_data(bloom: &BloomFilterData, script: Script) -> bool {
	for instruction in script.iter() {
		match instruction {
			Err(_) => break,
			Ok(instruction) => {
				if let Some(instruction_data) = instruction.data {
					if bloom.contains(&*instruction_data) {
						return true;
					}
				}
			},
		}
	}

	false
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::iter::repeat;
	use chain::IndexedTransaction;
	use message::types;
	use primitives::bytes::Bytes;
	use primitives::hash::H256;
	use ser::serialize;
	use super::{BloomFilter, BloomFilterData};

	fn default_filterload() -> types::FilterLoad {
		types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		}
	}

	fn make_filteradd(data: &[u8]) -> types::FilterAdd {
		types::FilterAdd {
			data: data.into(),
		}
	}

	#[test]
	fn bloom_insert_data() {
		let mut bloom = BloomFilterData::with_filter_load(default_filterload());

		assert!(!bloom.contains(&*H256::default()));

		bloom.insert(&*H256::default());
		assert!(bloom.contains(&*H256::default()));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_hash() {
		let tx1: IndexedTransaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: IndexedTransaction = test_data::TransactionBuilder::with_output(20).into();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&*tx1.hash));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_output_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// output script: OP_DUP OP_HASH160 380cb3c594de4e7e9b8e18db182987bebb5a4f70 OP_EQUALVERIFY OP_CHECKSIG
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_out_data: Bytes = "380cb3c594de4e7e9b8e18db182987bebb5a4f70".into();
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_out_data));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_previous_output_point() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_previous_output: Bytes = serialize(&tx1.raw.inputs[0].previous_output);
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_previous_output));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn connection_filter_matches_transaction_by_input_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// input script: PUSH DATA 304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_input_data: Bytes = "304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01".into();
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_input_data));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn bloom_filter_data_works_on_address() {
		use message::{types, deserialize_payload};

		// real world message
		let payload: Bytes = "fd77078732c06c257f2818a0d14738804d6a18a0f0a9800e039be473286d591f48040c99e96d2e0cd8480b03c0823fdd79310e0e04c86314ae24cabaa931f39852c813172e524c4b231020557dda2023130b0f1f309de822014bf6d4305d238ea013b8539e9c138053076a22020335bc1042e543f260787993a07a155a7a560c1475f75ec109b73822491c5880924303a5545ffc465132960fe40a8822010f919c38910f31c546be6940090f0476a02570de8d28258ee8081dea073829386a9de01b0070348f0f2fb48c6a1b4e92292de7b60dbafb8df3a6cab6a824e50005744018b384f080c8265152e406b1c85906d5325d1c83ac880d02214ad401ddc07657002c47708e338020bd14fcc147bfe49335647062074a4d3276e0d3a5110826a05d4fb025129f48c26e5819dd878d851b84802b7a211097813ef952410390c95bb294a7a8faca3667f0068489a69d9c9e8405c035442874a6a8c448c60600834a22041ce36a8065b086469bbb1b46d326b7a9923054ad4e32e4b7231aa203c5acab1f5821b92d2f00728819e587e1e6ff9fa6e66ff52fb54bce3648a7b043cbd19469aa5af0891eb4979def822f06522f080830b411545b4240e01195b0f0962e628050f0f8290269c45c20aa16559d17ceca68b81a21370909a1086614531577600ad8480ae1023c9173260bc6385c0e2d46806c05401a17ac969edfe65fd4000007e1c8a13ac651922edfa5a3235a0bcc10cc0067b4404d64260aea022391b007d3010c8380fda9c821d37258e47a0ab6485baff88876ba55cd190edf730970875f75522010e40f8c28e4872a2065c96390041bef9273a6a1c05d2bd28193b205b2cf25e44c5290926a4d392450b3135a0db7cc2c0696384eee68a65a4280ac3c44207264243fd18795c69588f4e418a26d4e21d1cee894325a2fc869b03fec68fd3e86b31998f9f9a6402dd672227451b1b5419a89c90ae96b9292a1ca83848bc026dcd00ec740342496730a7ab8e48200c7ea240d0e34a07890a9203938ee188475b3d6dd2f50399019c249536955894917fa3acc400ce04ec42d5e89e2d48aa085ae129226d0ea2a4d0038db88fd5ec0688768cea449171280438e8f5164d8682c40b91a2dab042800b1f312b22460e4905dccee59842a2bc9f807d542e08388fec013696d281c0356880040b0610ac4cb148a95a5924875891b2217040bbab7bba21f14898203ae87153206601c20c484f072216c1714ac5ded41a6d213fc09962f195129ac09d85dc05501a773163646021100481cb385a0fdb1609e8d1bc042942f169884e60482ae2004924fc2eb0b7061855858b5e54a4517352084778f38c09f1f004431da4531059c1296436d4c89e8839d34506a27b07c94fa80d2a0a9b73208a79982f0a5d16e0b723d38816a6a666c09bceae5a46d8a032248302224ae43c7e4801004ea6671c6d08142b67e27269a4ac6418748d74f481fc2dbbac5852afc645026cb462f790347d32a26e0b88209f2168110eb4e88394dd0f3bc27958994803238059d581d8cda73493a994c433fad902e20093ad40b1570543af928bc4830c3976d2802635400c0ac5a25833008f00b8bb691c1d9ce026373d05ac03fa6851402e05c6f109fa1754da54a20aa9f4f4782e68966a5113ae141d495d2a53d64bb6a023616b243e0333e2e0c65f2078559a98f48e8637f7b2ff326572a32532e2ec9e9651c092c52a522a5120249a159fa49d56d304806b502c425e3981275409f38b20418bc8206d21e884401c05c3c7d0b7404cf2a97706092b5a818638122ce750ce8780812900f501f4f02cb90fd5ea615e611006e010920072d08ca01b535741460c9aa1ab567ac2f79a004400e523fdcf95320bc08a37f54aaa01b2a2d0aa343ec13205131124b445a2c1b9a7542c63c6ced549447462099c12a9c613838995d718849650300056845311b2c93a1d35a66622466e0a3cef68594faa11021751c0e5358027721d2a2362fc9353655680c80fa5cad35685a0454932251aa121cb50583ae987c2a6e8009f3342048019509760b7e233de62cdcb636344d8a012af9f61a539ec66801c46a56b3d8ea6825e95430d1cc71fcc0bc977e4ea27f83e284cab0aea0a5085e67039901252a722054e33168a89d160a5908a325b63654016da1e94450548bac84a1b22fcc92ce7e2018967755711480e9048f5ded20b5d3960f21b559f1a0be84a53721f7b0f283d7d1bc2bd7f5d7550d3213814eb56e13f0b106acc07b05ece2c518117e934409843f1f889c2d84845c540514badb4ca00864e8bde78e50b0837478104c018cd5996977196e4f064002480a2761489000984d44d0077df65696f306930c893c50b08516e9ea82e02eb0400bc3d1adade23161b45e5210e08e981568f8af232bdf0f3c460349a8800c9f2c510eecc8ccee39e8b0898d329560aaf4d9594a551186423f79aa6806ce8c541506283a54e8859a840814012cad0289627f8659658218f6e58926af0849b4b23b40ac76280061b90c940f71617e0397ea145968250b1060608e4002432021195635dc52e0495c69fa67768a4a89ec32206fa30f62a85503de8c79df940f808c0de2f1723c0c84d89f317c4c1287a40759946d9cc8c43044a817dd6bb8ed326e4ab800fd8815482910de3cc360d40080a8d956e049ec6d1000000000943a3102".parse().unwrap();
		let message: types::FilterLoad = deserialize_payload(&payload, 70001).unwrap();
		// containing this address
		let address: Bytes = "BA99435B59DEBDBB5E207C0B33FF55752DBC5EFF".parse().unwrap();

		let bloom = BloomFilterData::with_filter_load(message);
		assert!(bloom.contains(address.as_slice()));
	}
}
