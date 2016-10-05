use std::thread;
use std::net::SocketAddr;
use futures::{Future, BoxFuture};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use net::{connect, listen};
use {Config, Error};

pub fn run(config: Config, handle: &Handle) -> Result<BoxFuture<(), Error>, Error> {
	for seednode in config.seednodes.clone().into_iter() {
		let socket = SocketAddr::new(seednode, config.connection.magic.port());
		let connection = connect(&socket, &handle, &config.connection);
		thread::spawn(move || {
			match connection.wait() {
				Ok(_connection) => {
					println!("Connected to seednode {}", seednode);
				},
				Err(err) => {
					println!("Connection failed {:?}", err);
				}
			}
		});
	}

	let listen = try!(listen(&handle, config.connection));
	let server = listen.for_each(|connection| {
		println!("new connection: {:?}", connection.version);
		Ok(())
	}).boxed();

	Ok(server)
}
