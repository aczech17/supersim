enum CPUPhase
{
    Fetch,
    DecodeAndExecute,
    Memory,
}

pub(super) struct MemoryBuffer
{
    address: u32,
    data: u32,
    data_size: u8,
    store: bool,
    write_back_register: u8,
    sign_extended: bool,
}

pub(super) struct CPU
{
    int_reg: [u32; 32],
    fp_reg: [f32; 32],
    hi: u32,
    lo: u32,
    pc: u32,
    memory_buffer: MemoryBuffer,
    phase: CPUPhase,
}

impl CPU
{
    pub fn new() -> CPU
    {
        let memory_buffer = MemoryBuffer
        {
            address: 0,
            data: 0,
            data_size: 0,
            store: false,
            write_back_register: 0,
            sign_extended: false,
        };

        CPU
        {
            int_reg: [0; 32],
            fp_reg: [0.0; 32],
            hi: 0,
            lo: 0,
            pc: 0,
            memory_buffer,
            phase: CPUPhase::Fetch,
        }
    }

    fn write_to_reg(&mut self, reg_num: u8, val: u32)
    {
        let reg_num = reg_num as usize;
        match reg_num
        {
            0 => {},
            1..=31 => self.int_reg[reg_num] = val,
            _ => panic!("Bad register number"),
        };
    }
}

impl CPU // opcodes
{
    fn sll(&mut self, rd: u8, rt: u8, shamt: u8)
    {
        let op1 = self.int_reg[rt as usize];
        let op2 = shamt & 0b11111;

        let result = op1 << op2;
        self.write_to_reg(rd, result);
    }

    fn srl(&mut self, rd: u8, rt: u8, shamt: u8)
    {
        let op1 = self.int_reg[rt as usize];
        let op2 = shamt & 0b11111;

        let result = op1 >> op2;
        self.write_to_reg(rd, result);
    }

    fn sra(&mut self, rd: u8, rt: u8, shamt: u8)
    {
        let op1 = self.int_reg[rt as usize] as i32;
        let op2 = shamt & 0b11111;

        let result = (op1 >> op2) as u32;
        self.write_to_reg(rd, result);
    }

    fn sllv(&mut self, rd: u8, rt: u8, rs: u8)
    {
        let op1 = self.int_reg[rt as usize];
        let op2 = self.int_reg[rs as usize];

        let result = op1 << op2;
        self.write_to_reg(rd, result);
    }

    fn srlv(&mut self, rd: u8, rt: u8, rs: u8)
    {
        let op1 = self.int_reg[rt as usize];
        let op2 = self.int_reg[rs as usize];

        let result = op1 >> op2;
        self.write_to_reg(rd, result);
    }

    fn srav(&mut self, rd: u8, rt: u8, rs: u8)
    {
        let op1 = self.int_reg[rt as usize] as i32;
        let op2 = self.int_reg[rs as usize] as i32;

        let result = (op1 >> op2) as u32;
        self.write_to_reg(rd, result);
    }

    fn jr(&mut self, rs: u8)
    {
        self.pc = self.int_reg[rs as usize];
    }

    fn jalr(&mut self, rd: u8, rs: u8)
    {
        self.write_to_reg(rd, self.pc);
        self.pc = self.int_reg[rs as usize];
    }

    fn syscall()
    {
        // TODO
    }

    fn mfhi(&mut self, rd: u8)
    {
        self.write_to_reg(rd, self.hi);
    }

    fn mthi(&mut self, rs: u8)
    {
        self.hi = self.int_reg[rs as usize];
    }

    fn mflo(&mut self, rd: u8)
    {
        self.write_to_reg(rd, self.lo);
    }

    fn mtlo(&mut self, rs: u8)
    {
        self.lo = self.int_reg[rs as usize];
    }

    fn mult(&mut self, rs: u8, rt: u8) // signed multiplication
    {
        let op1 = self.int_reg[rs as usize] as u64 as i64;
        let op2 = self.int_reg[rt as usize] as u64 as i64;

        let result = (op1 * op2) as u64;

        let high = (result >> 32) as u32;
        let low = (result & 0xFFFFFFFF) as u32;

        self.hi = high;
        self.lo = low;
    }

    fn multu(&mut self, rs: u8, rt: u8) // unsigned multiplication
    {
        let op1 = self.int_reg[rs as usize] as u64;
        let op2 = self.int_reg[rt as usize] as u64;

        let result = op1 * op2;

        let high = (result >> 32) as u32;
        let low = (result >> 32) as u32;

        self.hi = high;
        self.lo = low;
    }

    fn div(&mut self, rs: u8, rt: u8) // signed division
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = self.int_reg[rt as usize] as i32;

        let quotient = (op1 / op2) as u32;
        let modulo = (op1 % op2) as u32;

        self.lo = quotient;
        self.hi = modulo;
    }

    fn divu(&mut self, rs: u8, rt: u8) // unsigned division
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let quotient = op1 / op2;
        let modulo = op1 % op2;

        self.lo = quotient;
        self.hi = modulo;
    }

    fn add(&mut self, rd: u8, rs: u8, rt: u8) // signed division with exception on overflow
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = self.int_reg[rt as usize] as i32;

        let result = op1 + op2;

        // overflow check
        if (op1 > 0 && op2 > 0 && result < 0) | (op1 < 0 && op2 < 0 && result > 0)
        {
            self.set_exception();
        }

        self.write_to_reg(rd, result as u32);
    }

    fn addu(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = op1 + op2;

        self.write_to_reg(rd, result);
    }

    fn sub(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = self.int_reg[rt as usize] as i32;

        let result = op1 - op2;

        if (op1 < 0 && op2 > 0 && result > 0) || (op1 > 0 && op2 < 0 && result < 0)
        {
            self.set_exception();
        }

        self.write_to_reg(rd, result as u32);
    }

    fn subu(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = op1 - op2;

        self.write_to_reg(rd, result);
    }

    fn and(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = op1 & op2;

        self.write_to_reg(rd, result);
    }

    fn or(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = op1 | op2;

        self.write_to_reg(rd, result);
    }

    fn xor(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = op1 ^ op2;

        self.write_to_reg(rd, result);
    }

    fn nor(&mut self, rd: u8, rs: u8, rt: u8)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = !(op1 | op2);

        self.write_to_reg(rd, result);
    }

    fn slt(&mut self, rd: u8, rs: u8, rt: u8) // signed comparison
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = self.int_reg[rt as usize] as i32;

        let difference = op1 - op2;

        let result = if difference < 0
        {1}
        else
        {0};

        self.write_to_reg(rd, result);
    }

    fn sltu(&mut self, rd: u8, rs: u8, rt: u8) // unsigned comparison
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = self.int_reg[rt as usize];

        let result = if op1 < op2
        { 1 }
        else {0};

        self.write_to_reg(rd, result);
    }

    fn j(&mut self, address: u32)
    {
        let address = address & 0x_03_FF_FF_FF; // lower 26 bits
        let upper = self.pc << 28;
        let lower = address << 2;

        let new_address = upper | lower;
        self.pc = new_address;
    }

    fn jal(&mut self, address: u32)
    {
        const RETURN_ADDRESS_REG: u8 = 31;
        self.write_to_reg(RETURN_ADDRESS_REG, self.pc);

        self.j(address);
    }

    fn beq(&mut self, rs: u8, rt: u8, imm: u16)
    {
        if self.int_reg[rs as usize] == self.int_reg[rt as usize]
        {
            self.branch(imm);
        }
    }

    fn bne(&mut self, rs: u8, rt: u8, imm: u16)
    {
        if self.int_reg[rs as usize] != self.int_reg[rt as usize]
        {
            self.branch(imm);
        }
    }

    fn blez(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] <= 0
        {
            self.branch(imm);
        }
    }

    fn bgtz(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] > 0
        {
            self.branch(imm);
        }
    }

    fn addi(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = imm as i16 as i32;

        let result = op1 + op2;

        if (op1 < 0 && op2 < 0 && result > 0) || (op1 > 0 && op2 > 0 && result < 0)
        {
            self.set_exception();
        }

        self.write_to_reg(rt, result as u32);
    }

    fn addiu(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = imm as i16 as i32 as u32; // sign extended

        let result = op1 + op2;

        self.write_to_reg(rt, result);
    }

    fn slti(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize] as i32;
        let op2 = imm as i16 as i32;

        let result = if op1 < op2 {1} else {0};
        self.write_to_reg(rt, result);
    }

    fn sltiu(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = imm as i16 as i32 as u32;

        let result = if op1 < op2 {1} else {0};
        self.write_to_reg(rt, result);
    }

    fn andi(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = imm as u32;

        let result = op1 & op2;
        self.write_to_reg(rt, result);
    }

    fn ori(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = imm as u32;

        let result = op1 | op2;
        self.write_to_reg(rt, result);
    }

    fn xori(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let op1 = self.int_reg[rs as usize];
        let op2 = imm as u32;

        let result = op1 ^ op2;
        self.write_to_reg(rt, result);
    }

    fn lui(&mut self, rt: u8, imm: u16)
    {
        let result = (imm as u32) << 16;
        self.write_to_reg(rt, result);
    }

    fn lb(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);
        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 1,
            store: false,
            write_back_register: rt,
            sign_extended: false,
        };
    }

    fn lh(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);
        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 2,
            store: false,
            write_back_register: rt,
            sign_extended: true,
        }
    }

    fn lw(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);
        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 4,
            store: false,
            write_back_register: rt,
            sign_extended: true, // whatever
        }
    }

    fn lbu(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);
        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 1,
            store: false,
            write_back_register: rt,
            sign_extended: false,
        }
    }

    fn lhu(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);
        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 2,
            store: false,
            write_back_register: rt,
            sign_extended: false,
        }
    }

    fn sb(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let data = self.int_reg[rt as usize] & 0xFF;
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);

        self.memory_buffer = MemoryBuffer
        {
            address,
            data,
            data_size: 1,
            store: true,
            write_back_register: 0,
            sign_extended: false,
        }
    }

    fn sh(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let data = self.int_reg[rt as usize] & 0xFFFF;
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);

        self.memory_buffer = MemoryBuffer
        {
            address,
            data,
            data_size: 2,
            store: true,
            write_back_register: 0,
            sign_extended: false,
        }
    }

    fn sw(&mut self, rt: u8, rs: u8, imm: u16)
    {
        let data = self.int_reg[rt as usize];
        let address = self.int_reg[rs as usize] + (imm as i16 as i32 as u32);

        self.memory_buffer = MemoryBuffer
        {
            address,
            data,
            data_size: 4,
            store: true,
            write_back_register: 0,
            sign_extended: false,
        }
    }
}

impl CPU // auxiliary
{
    fn branch(&mut self, imm: u16)
    {
        let offset = ((imm as i16) * 4) as i32;
        let new_pc = (self.pc as i32 + offset) as u32;
        self.pc = new_pc;
    }

    fn set_exception(&mut self)
    {
        // todo
    }
}
