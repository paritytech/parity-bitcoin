use std::{thread, io};
use std::net::SocketAddr;
use futures::{Future, BoxFuture};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use net::{connect, listen};
use Config;

pub fn run(config: Config, handle: &Handle) -> Result<BoxFuture<(), io::Error>, io::Error> {
	for seednode in config.seednodes.clone().into_iter() {
		let socket = SocketAddr::new(seednode, config.connection.magic.port());
		let connection = connect(&socket, &handle, &config.connection);
		thread::spawn(move || {
			match connection.wait() {
				Ok(Ok(_connection)) => {
					println!("Connected to seednode {}", seednode);
				},
				Ok(Err(_err)) => {
					println!("Handshake failed");
				},
				Err(err) => {
					println!("Connection failed {:?}", err);
				}
			}
		});
	}

	let listen = try!(listen(&handle, config.connection));
	let server = listen.for_each(|connection| {
		match connection {
			Ok(connection) => {
				println!("new connection: {:?}", connection.version);
			},
			Err(_err) => {
				println!("handshake failed");
			}
		}
		Ok(())
	}).boxed();

	Ok(server)
}
