mod manual;
mod normal;
mod seednode;

pub enum SessionState {
	Connected,
	AcceptedConnection,
	SentGetAddr,
	Closing,
	Idle,
}
