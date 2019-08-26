use std::io;
use std::time::Duration;
use futures::{Future, Select, Poll, Async};
use tokio_core::reactor::{Handle, Timeout};

type DeadlineBox<F> = Box<dyn Future<Item = DeadlineStatus<<F as Future>::Item>, Error = <F as Future>::Error> + Send>;

pub fn deadline<F, T>(duration: Duration, handle: &Handle, future: F) -> Result<Deadline<F>, io::Error>
	where F: Future<Item = T, Error = io::Error> + Send + 'static, T: 'static {
	let timeout: DeadlineBox<F> = Box::new(try!(Timeout::new(duration, handle)).map(|_| DeadlineStatus::Timeout));
	let future: DeadlineBox<F> = Box::new(future.map(DeadlineStatus::Meet));
	let deadline = Deadline {
		future: timeout.select(future),
	};
	Ok(deadline)
}

pub enum DeadlineStatus<T> {
	Meet(T),
	Timeout,
}

pub struct Deadline<F> where F: Future + Send {
	future: Select<DeadlineBox<F>, DeadlineBox<F>>,
}

impl<F, T> Future for Deadline<F> where F: Future<Item = T, Error = io::Error> + Send {
	type Item = DeadlineStatus<T>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		match self.future.poll() {
			Ok(Async::Ready((result, _other))) => Ok(Async::Ready(result)),
			Ok(Async::NotReady) => Ok(Async::NotReady),
			Err((err, _other)) => Err(err),
		}
	}
}
