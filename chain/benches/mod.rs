#![feature(test)]

extern crate chain;
extern crate test;

#[cfg(test)]
mod benchmarks {
	use super::chain::{hash::H256, merkle_root};
	use super::test::Bencher;

	fn prepare_hashes(num: u32) -> Vec<H256> {
		let mut vec = Vec::new();
		let h1 = H256::from_reversed_str("1da63abbc8cc611334a753c4c31de14d19839c65b2b284202eaf3165861fb58d");
		let h2 = H256::from_reversed_str("26c6a6f18d13d2f0787c1c0f3c5e23cf5bc8b3de685dd1923ae99f44c5341c0c");
		let h3 = H256::from_reversed_str("d1bc8d3ba4afc7e109612cb73acbdddac052c93025aa1f82942edabb7deb82a1");
		for v in 0..num {
			match v % 3 {
				0 => vec.push(h1.clone()),
				1 => vec.push(h2.clone()),
				2 => vec.push(h3.clone()),
				_ => (),
			}
		}
		vec
	}

	#[bench]
	fn bench_merkle_root_with_5_hashes(b: &mut Bencher) {
		let vec = prepare_hashes(5);
		b.iter(|| merkle_root(&vec));
	}

	#[bench]
	fn bench_merkle_root_with_1000_hashes(b: &mut Bencher) {
		let vec = prepare_hashes(1000);
		b.iter(|| merkle_root(&vec));
	}
}
