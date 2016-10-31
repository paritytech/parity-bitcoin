use std::{io, fs, path};
use ser::{ReadIterator, deserialize_iterator, Error as ReaderError};
use block::Block;
use fs::{read_blk_dir, ReadBlkDir};

pub fn open_blk_file<P>(path: P) -> Result<BlkFile, io::Error> where P: AsRef<path::Path> {
	let file = try!(fs::File::open(path));
	let blk_file = BlkFile {
		reader: deserialize_iterator(file),
	};
	Ok(blk_file)
}

pub struct BlkFile {
	reader: ReadIterator<fs::File, Block>,
}

impl Iterator for BlkFile {
	type Item = Result<Block, ReaderError>;

	fn next(&mut self) -> Option<Self::Item> {
		self.reader.next()
	}
}

pub fn open_blk_dir<P>(path: P) -> Result<BlkDir, io::Error> where P: AsRef<path::Path> {
	let dir_iter = try!(read_blk_dir(path));
	let blk_dir = BlkDir {
		dir_iter: dir_iter,
		current_file: None,
	};
	Ok(blk_dir)
}

pub struct BlkDir {
	dir_iter: ReadBlkDir,
	current_file: Option<BlkFile>,
}

impl Iterator for BlkDir {
	type Item = Result<Block, ReaderError>;

	fn next(&mut self) -> Option<Self::Item> {
		// TODO: use chained iterators instead
		let next_file = match self.current_file {
			Some(ref mut file) => match file.next() {
				Some(block) => return Some(block),
				None => self.dir_iter.next(),
			},
			None => self.dir_iter.next(),
		};

		match next_file {
			Some(Ok(next_file)) => {
				self.current_file = match open_blk_file(next_file.path) {
					Err(_) => return Some(Err(ReaderError::MalformedData)),
					Ok(file) => Some(file),
				};
				self.next()
			},
			Some(Err(_)) => {
				Some(Err(ReaderError::MalformedData))
			},
			None => None,
		}
	}
}

