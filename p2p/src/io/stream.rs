use futures::stream::BoxStream;
use Error;

pub type IoStream<T> = BoxStream<T, Error>;
