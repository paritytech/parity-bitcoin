#[derive(Default, Clone)]
pub struct MessageStats {
	pub addr: u64,
	pub getdata: u64,
	pub getheaders: u64,
	pub headers: u64,
	pub reject: u64,
	pub tx: u64,
	pub inv: u64,
	pub ping: u64,
	pub pong: u64,
	pub verack: u64,
	pub version: u64,
}

#[derive(Default, Clone)]
pub struct PeerStats {
	pub last_send: u32,
	pub last_recv: u32,
	pub total_send: u64,
	pub total_recv: u64,
	pub avg_ping: f64,
	pub min_ping: f64,
	pub synced_blocks: u32,
	pub synced_headers: u32,
	pub counter_send: MessageStats,
	pub counter_recv: MessageStats,
	pub bytes_send: MessageStats,
	pub bytes_recv: MessageStats,
}

macro_rules! impl_send {
	($msg: ident, $method_name: ident) => {
		pub fn $method_name(&mut self, bytes: usize) {
			self.last_send = ::time::get_time().sec as u32;
			self.total_send += bytes as u64;
			self.bytes_send.$msg += bytes as u64;
			self.counter_send.$msg += bytes as u64;
		}
	}
}

macro_rules! impl_recv {
	($msg: ident, $method_name: ident) => {
		pub fn $method_name(&mut self, bytes: usize) {
			self.last_recv = ::time::get_time().sec as u32;
			self.total_recv += bytes as u64;
			self.bytes_recv.$msg += bytes as u64;
			self.counter_recv.$msg += bytes as u64;
		}
	}
}

macro_rules! impl_both {
	($msg: ident, $method_send: ident, $method_recv: ident) => {
		impl_send!($msg, $method_send);
		impl_recv!($msg, $method_recv);
	}
}

impl PeerStats {
	impl_both!(addr, send_addr, recv_addr);
	impl_both!(getdata, send_getdata, recv_getdata);
	impl_both!(getheaders, send_getheaders, recv_getheaders);
	impl_both!(headers, send_headers, recv_headers);
	impl_both!(reject, send_reject, recv_reject);
	impl_both!(tx, send_tx, recv_tx);
	impl_both!(inv, send_inv, recv_inv);
	impl_both!(ping, send_ping, recv_ping);
	impl_both!(pong, send_pong, recv_pong);
	impl_both!(verack, send_verack, recv_verack);
	impl_both!(version, send_version, recv_version);
}
