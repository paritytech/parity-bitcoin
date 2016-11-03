use std::{io, fs, path};
use ser::{ReadIterator, deserialize_iterator, Error as ReaderError};
use block::Block;
use fs::read_blk_dir;

pub fn open_blk_file<P>(path: P) -> Result<BlkFile, io::Error> where P: AsRef<path::Path> {
	trace!("Opening blk file: {:?}", path.as_ref());
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
	let iter = try!(read_blk_dir(path))
		// flatten results...
		.flat_map(|entry| entry.and_then(|file| open_blk_file(file.path)))
		// flat iterators over each block in each file
		.flat_map(|file| file);

	let blk_dir = BlkDir {
		iter: Box::new(iter),
	};

	Ok(blk_dir)
}

pub struct BlkDir {
	iter: Box<Iterator<Item = Result<Block, ReaderError>>>,
}

impl Iterator for BlkDir {
	type Item = Result<Block, ReaderError>;

	fn next(&mut self) -> Option<Self::Item> {
		self.iter.next()
	}
}

