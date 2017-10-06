//use bit_field::BitField;

use core::register::Reg;
use core::instruction::Op;

#[allow(non_snake_case)]
pub fn decode_BX(command: u16) -> Op {

    Op::BX { rm: Reg::from_u16((command >> 3) & 0xf).unwrap() }
}