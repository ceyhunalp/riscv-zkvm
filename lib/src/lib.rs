#![allow(unexpected_cfgs)]

use alloy_sol_types::sol;
use core::cmp::Ord;
use core::panic;

extern crate alloc;

type Reg = usize;

sol! {
    /// The public values encoded as a struct that can be easily deserialized inside Solidity.
    struct PublicValuesStruct {
        uint128 count;
    }
}

const SP: Reg = 2;
const A0: Reg = 10;
const A1: Reg = 11;
const A2: Reg = 12;

#[inline(always)]
const fn reg_decode(reg: u32) -> Reg {
    (reg & 0b11111) as Reg
}

#[inline(always)]
const fn sign_ext(value: u32, bits: u32) -> i32 {
    let mask = 1 << (bits - 1);
    (value ^ mask) as i32 - mask as i32
}

#[inline(always)]
const fn bits(start: u32, end: u32, value: u32, position: u32) -> u32 {
    let mask = (1 << (end - start + 1)) - 1;
    ((value >> position) & mask) << start
}

struct State {
    pc: u32,
    regs: [u32; 32],
    memory: alloc::vec::Vec<u8>,
}

enum Status {
    Error,
    Continue,
    Finished,
}

impl State {
    #[inline(always)]
    fn set_reg(&mut self, reg: Reg, value: u32) {
        if reg != 0 {
            self.regs[reg] = value;
        }
    }

    fn step(&mut self) -> Status {
        let op = &self.memory[self.pc as usize..self.pc as usize + 4];
        let op = u32::from_le_bytes([op[0], op[1], op[2], op[3]]);

        self.pc += 4;

        let dst = reg_decode(op >> 7);
        let src1 = reg_decode(op >> 15);
        let src2 = reg_decode(op >> 20);
        let funct3 = (op >> 12) & 0b111;

        if op == 0x00000073 {
            match self.regs[A0] {
                0x45584954 => return Status::Finished,
                1 => {
                    let pointer = self.regs[A1] as usize;
                    let length = self.regs[A2] as usize;
                    let _blob = &self.memory[pointer..pointer + length];

                    println!("guest> {}", String::from_utf8_lossy(_blob));
                    return Status::Continue;
                }
                _ => return Status::Error,
            }
        }

        match op & 0b1111111 {
            0b0110111 => {
                // LUI
                self.set_reg(dst, op & 0xfffff000);
                Status::Continue
            }
            0b0010111 => {
                // AUIPC
                self.set_reg(
                    dst,
                    ((self.pc - 4) as i32 + (op & 0xfffff000) as i32) as u32,
                );
                Status::Continue
            }
            0b1101111 => {
                // JAL
                self.set_reg(dst, self.pc);
                self.pc = (sign_ext(
                    bits(1, 10, op, 21)
                        | bits(11, 11, op, 20)
                        | bits(12, 19, op, 12)
                        | bits(20, 20, op, 31),
                    21,
                ) + (self.pc - 4) as i32) as u32;
                Status::Continue
            }
            0b1100111 if funct3 == 0 => {
                // JALR
                let target = (self.regs[src1] as i32 + sign_ext(op >> 20, 12) as i32) as u32 & !1;
                self.set_reg(dst, self.pc);
                self.pc = target;
                Status::Continue
            }
            0b1100011 => {
                let target = (sign_ext(
                    bits(1, 4, op, 8)
                        | bits(5, 10, op, 25)
                        | bits(11, 11, op, 7)
                        | bits(12, 12, op, 31),
                    13,
                ) + (self.pc - 4) as i32) as u32;

                let src1 = self.regs[src1];
                let src2 = self.regs[src2];
                let branch = match funct3 {
                    0b000 => src1 == src2,                   // BEQ
                    0b001 => src1 != src2,                   // BNE
                    0b100 => (src1 as i32) < (src2 as i32),  // BLT
                    0b101 => (src1 as i32) >= (src2 as i32), // BGE
                    0b110 => src1 < src2,                    // BLTU
                    0b111 => src1 >= src2,                   // BGEU
                    _ => return Status::Error,
                };

                if branch {
                    self.pc = target;
                }
                Status::Continue
            }
            0b0000011 => {
                let offset = sign_ext((op >> 20) & 0b111111111111, 12);
                let address = (self.regs[src1] as i32 + offset) as u32;
                let value = match funct3 {
                    0b000 => {
                        *self.memory.get(address as usize).ok_or(()).unwrap() as i8 as i32 as u32
                    }
                    0b001 => {
                        let slice = self
                            .memory
                            .get(
                                address as usize
                                    ..address.checked_add(2).ok_or(()).unwrap() as usize,
                            )
                            .ok_or(())
                            .unwrap();
                        i16::from_le_bytes([slice[0], slice[1]]) as i32 as u32
                    }
                    0b010 => {
                        let slice = self
                            .memory
                            .get(
                                address as usize
                                    ..address.checked_add(4).ok_or(()).unwrap() as usize,
                            )
                            .ok_or(())
                            .unwrap();
                        u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
                    }
                    0b100 => *self.memory.get(address as usize).ok_or(()).unwrap() as u32,
                    0b101 => {
                        let slice = self
                            .memory
                            .get(
                                address as usize
                                    ..address.checked_add(2).ok_or(()).unwrap() as usize,
                            )
                            .ok_or(())
                            .unwrap();
                        u16::from_le_bytes([slice[0], slice[1]]) as u32
                    }
                    _ => return Status::Error,
                };

                self.set_reg(dst, value);
                Status::Continue
            }
            0b0100011 => {
                let offset = sign_ext(
                    ((op >> (25 - 5)) & 0b111111100000) | ((op >> 7) & 0b11111),
                    12,
                );

                let src2 = self.regs[src2];
                let address = (self.regs[src1] as i32 + offset) as u32;

                match funct3 {
                    0b000 => *self.memory.get_mut(address as usize).ok_or(()).unwrap() = src2 as u8,
                    0b001 => self
                        .memory
                        .get_mut(
                            address as usize..address.checked_add(2).ok_or(()).unwrap() as usize,
                        )
                        .ok_or(())
                        .unwrap()
                        .copy_from_slice(&u16::to_le_bytes(src2 as u16)),
                    0b010 => self
                        .memory
                        .get_mut(
                            address as usize..address.checked_add(4).ok_or(()).unwrap() as usize,
                        )
                        .ok_or(())
                        .unwrap()
                        .copy_from_slice(&u32::to_le_bytes(src2)),
                    _ => return Status::Error,
                }
                Status::Continue
            }
            0b0010011 => {
                let src1 = self.regs[src1];
                let value = match funct3 {
                    0b001 => {
                        // SLLI
                        if op & 0xfe000000 != 0 {
                            return Status::Error;
                        }

                        let amount = bits(0, 4, op, 20) as u8;
                        src1 << amount
                    }
                    0b101 => {
                        let amount = bits(0, 4, op, 20) as u8;
                        match (op & 0xfe000000) >> 24 {
                            0b00000000 => src1 >> amount,                   // SRLI
                            0b01000000 => ((src1 as i32) >> amount) as u32, // SRAI
                            _ => return Status::Error,
                        }
                    }
                    0b000 => {
                        // ADDI
                        let imm = sign_ext(op >> 20, 12);
                        src1.wrapping_add(imm as u32)
                    }
                    0b010 => {
                        // SLTI
                        let imm = sign_ext(op >> 20, 12);
                        if (src1 as i32) < (imm as i32) {
                            1
                        } else {
                            0
                        }
                    }
                    0b011 => {
                        // SLTIU
                        let imm = sign_ext(op >> 20, 12);
                        if src1 < imm as u32 {
                            1
                        } else {
                            0
                        }
                    }
                    0b100 => {
                        // XORI
                        let imm = sign_ext(op >> 20, 12);
                        src1 ^ imm as u32
                    }
                    0b110 => {
                        // ORI
                        let imm = sign_ext(op >> 20, 12);
                        src1 | imm as u32
                    }
                    0b111 => {
                        // ANDI
                        let imm = sign_ext(op >> 20, 12);
                        src1 & imm as u32
                    }
                    _ => return Status::Error,
                };

                self.set_reg(dst, value);
                Status::Continue
            }
            0b0110011 => {
                let src1 = self.regs[src1];
                let src2 = self.regs[src2];

                let value = match op & 0b1111111_00000_00000_111_00000_0000000 {
                    0b0000000_00000_00000_000_00000_0000000 => src1.wrapping_add(src2), // ADD
                    0b0100000_00000_00000_000_00000_0000000 => src1.wrapping_sub(src2), // SUB
                    0b0000000_00000_00000_001_00000_0000000 => {
                        src1.wrapping_shl(src2) // SLL
                    }
                    0b0000000_00000_00000_010_00000_0000000 => {
                        // SLT
                        if (src1 as i32) < (src2 as i32) {
                            1
                        } else {
                            0
                        }
                    }
                    0b0000000_00000_00000_011_00000_0000000 => {
                        // SLTU
                        if src1 < src2 {
                            1
                        } else {
                            0
                        }
                    }
                    0b0000000_00000_00000_100_00000_0000000 => src1 ^ src2, // XOR
                    0b0000000_00000_00000_101_00000_0000000 => src1.wrapping_shr(src2), // SRL
                    0b0100000_00000_00000_101_00000_0000000 => ((src1 as i32) >> src2) as u32, // SRA
                    0b0000000_00000_00000_110_00000_0000000 => src1 | src2,                    // OR
                    0b0000000_00000_00000_111_00000_0000000 => src1 & src2, // AND
                    _ => return Status::Error,
                };

                self.set_reg(dst, value);
                Status::Continue
            }
            _ => {
                println!("ERROR: Unknown instruction encountered: 0x{:x}", op);
                Status::Error
            }
        }
    }
}

pub fn riscv(input_data: &Vec<u8>) -> u128 {
    let mut data = input_data.to_vec();
    let bss_size = {
        let xs = &data[data.len() - 4..];
        u32::from_le_bytes([xs[0], xs[1], xs[2], xs[3]]).min(16 * 1024 * 1024)
    };
    let mut length = data.len() - 4;
    data.truncate(length);
    length += bss_size as usize;
    data.resize(length, 0);

    let mut vm = State {
        pc: 8,
        regs: [0; 32],
        memory: data,
    };
    vm.regs[SP] = vm.memory.len() as u32;
    let mut count = 0;
    loop {
        count += 1;
        match vm.step() {
            Status::Continue => {}
            Status::Error => panic!(),
            Status::Finished => break,
        }
    }
    count
}
