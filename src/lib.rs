//! Ethcore's bitcoin library.

extern crate byteorder;
extern crate rustc_serialize;

pub mod block_header;
pub mod compact_integer;
pub mod stream;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
