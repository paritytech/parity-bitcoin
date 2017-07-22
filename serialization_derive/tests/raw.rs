extern crate serialization;
#[macro_use]
extern crate serialization_derive;

use serialization::{serialize, deserialize};

#[derive(Debug, PartialEq, Serializable, Deserializable)]
struct Foo {
	a: u8,
	b: u16,
	c: u32,
	d: u64,
}

#[derive(Debug, PartialEq, Serializable, Deserializable)]
struct Bar {
	a: Vec<Foo>,
}

#[test]
fn test_foo_serialize() {
	let foo = Foo {
		a: 1,
		b: 2,
		c: 3,
		d: 4,
	};

	let expected = vec![
		1u8,
		2, 0,
		3, 0, 0, 0,
		4, 0, 0, 0, 0, 0, 0, 0,
	].into();

	let result = serialize(&foo);
	assert_eq!(result, expected);

	let d = deserialize(expected.as_ref()).unwrap();
	assert_eq!(foo, d);
}

#[test]
fn test_bar_serialize() {
	let foo = Foo {
		a: 1,
		b: 2,
		c: 3,
		d: 4,
	};

	let foo2 = Foo {
		a: 5,
		b: 6,
		c: 7,
		d: 8,
	};

	let expected = vec![
		// number of items
		2u8,
		// first
		1,
		2, 0,
		3, 0, 0, 0,
		4, 0, 0, 0, 0, 0, 0, 0,
		// second
		5,
		6, 0,
		7, 0, 0, 0,
		8, 0, 0, 0, 0, 0, 0, 0,
	].into();

	let bar = Bar {
		a: vec![foo, foo2],
	};

	let result = serialize(&bar);
	assert_eq!(result, expected);

	let d = deserialize(expected.as_ref()).unwrap();
	assert_eq!(bar, d);
}
