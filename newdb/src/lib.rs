extern crate rocksdb;
extern crate byteorder;
extern crate elastic_array;
extern crate parking_lot;
#[macro_use] extern crate log;

mod kvdb;
mod transaction;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
