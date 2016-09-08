use script::{Opcode, Script, Num};

#[derive(Default)]
pub struct Builder {
	data: Vec<u8>,
}

impl Builder {
	pub fn push_opcode(mut self, opcode: Opcode) -> Self {
		self.data.push(opcode as u8);
		self
	}

	pub fn push_bool(mut self, value: bool) -> Self {
		let opcode = match value {
			true => Opcode::OP_1,
			false => Opcode::OP_0,
		};
		self.data.push(opcode as u8);
		self
	}

	pub fn push_num(mut self, num: Num) -> Self {
		self.push_data(&num.to_vec())
	}

	pub fn push_data(mut self, data: &[u8]) -> Self {
		let len = data.len();
		if len < Opcode::OP_PUSHDATA1 as usize {
			self.data.push(len as u8);
		} else if len < 0x100 {
			self.data.push(Opcode::OP_PUSHDATA1 as u8);
			self.data.push(len as u8);
		} else if len < 0x10000 {
			self.data.push(Opcode::OP_PUSHDATA2 as u8);
			self.data.push(len as u8);
			self.data.push((len >> 8) as u8);
		} else if len < 0x1_0000_0000 {
			self.data.push(Opcode::OP_PUSHDATA4 as u8);
			self.data.push(len as u8);
			self.data.push((len >> 8) as u8);
			self.data.push((len >> 16) as u8);
			self.data.push((len >> 24) as u8);
		} else {
			panic!("Cannot push more than 0x1_0000_0000 bytes");
		}

		self.data.extend_from_slice(data);
		self
	}

	pub fn into_script(self) -> Script {
		Script::new(self.data)
	}
}
