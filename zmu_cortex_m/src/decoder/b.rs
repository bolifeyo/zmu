use crate::core::bits::*;
use crate::core::condition::Condition;
use crate::core::instruction::Instruction;
use crate::core::operation::build_imm_10_11;
use crate::core::operation::build_imm_6_11;
use crate::core::operation::sign_extend;
use crate::core::ThumbCode;

#[allow(non_snake_case)]
#[inline]
pub fn decode_B_t1_SVC_t1(opcode: u16) -> Instruction {
    let cond = opcode.get_bits(8, 11);
    if cond == 0b1111 {
        return Instruction::SVC {
            imm32: bits_0_8(opcode) as u32,
        };
    }
    if cond == 0b1110 {
        return Instruction::UDF {
            imm32: 0,
            opcode: ThumbCode::from(opcode),
        };
    }

    Instruction::B {
        cond: Condition::from_u16(cond).unwrap(),
        imm32: sign_extend((bits_0_8(opcode) as u32) << 1, 8, 32) as i32,
        thumb32: false,
    }
}

#[allow(non_snake_case)]
#[inline]
pub fn decode_B_t2(opcode: u16) -> Instruction {
    Instruction::B {
        cond: Condition::AL,
        imm32: sign_extend((opcode.get_bits(0, 10) as u32) << 1, 11, 32) as i32,
        thumb32: false,
    }
}

#[allow(non_snake_case)]
#[inline]
pub fn decode_B_t3(opcode: u32) -> Instruction {
    let cond: u16 = opcode.get_bits(22, 25);

    let imm = build_imm_6_11(opcode);

    match Condition::from_u16(cond) {
        Some(c) => Instruction::B {
            cond: c,
            imm32: imm as i32,
            thumb32: true,
        },
        None => Instruction::B {
            cond: Condition::AL,
            imm32: imm as i32,
            thumb32: true,
        },
    }
}

#[allow(non_snake_case)]
#[inline]
pub fn decode_B_t4(opcode: u32) -> Instruction {
    let imm = build_imm_10_11(opcode);

    Instruction::B {
        cond: Condition::AL,
        imm32: imm as i32,
        thumb32: true
    }
}
