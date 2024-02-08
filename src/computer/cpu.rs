enum CPUPhase
{
    Fetch,
    DecodeAndExecute,
    WriteBack,
}

#[derive(Copy, Clone)]
pub(super) struct MemoryBuffer
{
    pub(super) address: u32,
    pub(super) data: u32,
    pub(super) data_size: u8,
    pub(super) store: bool,
    pub(super) write_back_register: u8,
    pub(super) sign_extended: bool,
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

    pub(super) fn tick(&mut self, data: u32) -> MemoryBuffer
    {
        match self.phase
        {
            CPUPhase::Fetch =>
            {
                self.memory_buffer = MemoryBuffer
                {
                    address: self.pc,
                    data: 0,
                    data_size: 4,
                    store: false,
                    write_back_register: 0,
                    sign_extended: false,
                };
                self.pc += 4;
                self.phase = CPUPhase::DecodeAndExecute;
            }
            CPUPhase::DecodeAndExecute =>
            {
                self.decode_and_execute(data);
                if self.memory_buffer.write_back_register == 0
                {
                    self.memory_buffer.data_size = 0;
                }
                self.phase = CPUPhase::WriteBack;
            }
            CPUPhase::WriteBack =>
            {
                if self.memory_buffer.data_size > 0
                {
                    self.memory_buffer.data = data; // save data taken from ram
                    self.write_back();
                }
                self.memory_buffer.data_size = 0; // reset the buffer
                self.phase = CPUPhase::Fetch;
            }
        }

        self.memory_buffer
    }

    fn decode_and_execute(&mut self, instruction: u32)
    {

        let opcode = instruction >> 26;
        let rs = ((instruction >> 21) & 0b11111) as u8;
        let rt = ((instruction >> 16) & 0b11111) as u8;
        let rd = ((instruction >> 11) & 0b11111) as u8;
        let shamt = ((instruction >> 6) & 0b11111) as u8;
        let funct = (instruction & 0b111111) as u8;
        let imm = (instruction & 0xFFFF) as u16;
        let address = instruction & 0x3FFFFFF;

        match (opcode, funct)
        {
            (0, 0) => self.sll(rd, rt, shamt),
            (0, 2) => self.srl(rd, rt, shamt),
            (0, 3) => self.sra(rd, rt, shamt),
            (0, 4) => self.sllv(rd, rt, rs),
            (0, 6) => self.srlv(rd, rt, rs),
            (0, 7) => self.srav(rd, rt, rs),
            (0, 8) => self.jr(rs),
            (0, 9) => self.jalr(rd, rs),
            (0, 12) => self.syscall(),
            (0, 16) => self.mfhi(rd),
            (0, 17) => self.mthi(rs),
            (0, 18) => self.mflo(rd),
            (0, 19) => self.mtlo(rs),
            (0, 24) => self.mult(rs, rt),
            (0, 25) => self.multu(rs, rt),
            (0, 26) => self.div(rs, rt),
            (0, 27) => self.divu(rs, rt),
            (0, 32) => self.add(rd, rs, rt),
            (0, 33) => self.addu(rd, rs, rt),
            (0, 34) => self.sub(rd, rs, rt),
            (0, 35) => self.subu(rd, rs, rt),
            (0, 36) => self.and(rd, rs, rt),
            (0, 37) => self.or(rd, rs, rt),
            (0, 38) => self.xor(rd, rs, rt),
            (0, 39) => self.nor(rd, rs, rt),
            (0, 42) => self.slt(rd, rs, rt),
            (0, 43) => self.sltu(rd, rs, rt),
            (2, _) => self.j(address),
            (3, _) => self.jal(address),
            (4, _) => self.beq(rs, rt, imm),
            (5, _) => self.bne(rs, rt, imm),
            (6, _) => self.blez(rs, imm),
            (7, _) => self.bgtz(rs, imm),
            (8, _) => self.addi(rt, rs, imm),
            (9, _) => self.addiu(rt, rs, imm),
            (10, _) => self.slti(rt, rs, imm),
            (11, _) => self.sltiu(rt, rs, imm),
            (12, _) => self.andi(rt, rs, imm),
            (13, _) => self.ori(rt, rs, imm),
            (14, _) => self.xori(rt, rs, imm),
            (15, _) => self.lui(rt, imm),
            (32, _) => self.lb(rt, rs, imm),
            (33, _) => self.lh(rt, rs, imm),
            (34, _) => self.lw(rt, rs, imm),
            (36, _) => self.lbu(rt, rs, imm),
            (37, _) => self.lhu(rt, rs, imm),
            (40, _) => self.sb(rt, rs, imm),
            (41, _) => self.sh(rt, rs, imm),
            (43, _) => self.sw(rt, rs, imm),
            _ => panic!("Bad instruction"),
        }
    }

    fn write_back(&mut self)
    {
        let data = self.memory_buffer.data;
        let result = if !self.memory_buffer.sign_extended {data}
        else
        {
            match self.memory_buffer.data_size
            {
                4 => data,
                2 => (data & 0xFFFF) as u16 as i16 as i32 as u32,
                1 => (data & 0xFF) as u8 as i8 as i32 as u32,
                _ => panic!("Bad data size"),
            }
        };

        let register = self.memory_buffer.write_back_register;
        self.write_to_reg(register, result);
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

    fn syscall(&mut self)
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
        let upper = self.pc & 0xF0000000; // upper 4 bits
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
