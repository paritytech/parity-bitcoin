extern crate primitives;
extern crate serialization as ser;
extern crate chain;

mod blk;
mod block;
mod fs;

pub use primitives::{hash, bytes};

pub use blk::{open_blk_file, open_blk_dir};
