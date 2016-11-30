use std::{io, fs, path, cmp};

/// Creates an iterator over all blk .dat files
pub fn read_blk_dir<P>(path: P) -> Result<ReadBlkDir, io::Error> where P: AsRef<path::Path> {
	let read_blk_dir = ReadBlkDir {
		read_dir: try!(fs::read_dir(path)),
	};

	Ok(read_blk_dir)
}

pub struct ReadBlkDir {
	read_dir: fs::ReadDir,
}

#[derive(PartialEq, Eq)]
pub struct BlkEntry {
	pub path: path::PathBuf,
}

impl cmp::PartialOrd for BlkEntry {
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		cmp::PartialOrd::partial_cmp(&self.path, &other.path)
	}
}

impl cmp::Ord for BlkEntry {
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		cmp::Ord::cmp(&self.path, &other.path)
	}
}

fn is_blk_file_name(file_name: &str) -> bool {
	if file_name.len() != 12 || !file_name.starts_with("blk") || !file_name.ends_with(".dat") {
		return false;
	}

	file_name[3..8].parse::<u32>().is_ok()
}

impl BlkEntry {
	fn from_dir_entry(dir_entry: fs::DirEntry) -> Option<Self> {
		match dir_entry.metadata() {
			Err(_) => return None,
			Ok(ref metadata) => {
				if !metadata.is_file() {
					return None;
				}
			}
		}

		match dir_entry.file_name().into_string() {
			Err(_) => return None,
			Ok(ref file_name) => {
				if !is_blk_file_name(file_name) {
					return None;
				}
			}
		}

		let entry = BlkEntry {
			path: dir_entry.path(),
		};

		Some(entry)
	}
}

impl Iterator for ReadBlkDir {
	type Item = Result<BlkEntry, io::Error>;

	fn next(&mut self) -> Option<Self::Item> {
		let mut result = None;
		while result.is_none() {
			match self.read_dir.next() {
				None => return None,
				Some(Err(err)) => return Some(Err(err)),
				Some(Ok(entry)) => {
					result = BlkEntry::from_dir_entry(entry);
				}
			}
		}
		result.map(Ok)
	}
}

#[cfg(test)]
mod test {
	use super::is_blk_file_name;

	#[test]
	fn test_is_blk_file_name() {
		assert!(is_blk_file_name("blk00000.dat"));
		assert!(is_blk_file_name("blk00232.dat"));
		assert!(!is_blk_file_name("blk0000.dat"));
		assert!(!is_blk_file_name("blk00000.daw"));
		assert!(!is_blk_file_name("blk00032.daw"));
		assert!(!is_blk_file_name("blk000ff.dat"));
	}
}
