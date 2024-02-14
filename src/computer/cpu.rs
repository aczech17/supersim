use std::mem;

enum CPUPhase
{
    Fetch,
    DecodeAndExecute,
    WriteBack,
    InterruptCheck,
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
    partial_write: Option<(usize, usize)>,
}

const EXCEPTION_HANDLER_ADDRESS: u32 = 0x8000_0180; // 0x8000_0080 ?

#[allow(unused)]
#[repr(u8)]
enum ExceptionCode
{
    Interrupt = 0,
    IllegalAddressLoad = 4,
    IllegalAddressStore = 5,
    BusErrorOnInstructionFetch = 6,
    BusErrorOnDataReference = 7,
    Syscall = 8,
    Break = 9,
    ReservedInstruction = 10,
    Overflow = 12,
    CalledTrap = 13, // https://faculty.kfupm.edu.sa/COE/aimane/coe301/lab/COE301_Lab_8_MIPS_Exceptions_and_IO.pdf
}

pub(super) struct CPU
{
    int_reg: [u32; 32],
    cp0_reg: [u32; 32],
    hi: u32,
    lo: u32,

    cp1_reg: [f32; 32],
    cc: [bool; 8],

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
            partial_write: None,
        };

        let mut cp0_reg = [0; 32];
        // status
        cp0_reg[12] = 0b0000000000000000_11111111_00___00_00_01;

        CPU
        {
            int_reg: [0; 32],

            cp0_reg,
            hi: 0,
            lo: 0,

            cp1_reg: [0.0; 32],
            cc: [false; 8],

            pc: 0,
            memory_buffer,
            phase: CPUPhase::Fetch,
        }
    }

    fn is_kernel_mode(&self) -> bool
    {
        self.cp0_reg[12] & 0b10 == 0
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

    pub(super) fn tick(&mut self, data: u32, interrupt_requests: u8) -> MemoryBuffer
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
                    partial_write: None,
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
                self.phase = CPUPhase::InterruptCheck;
            }
            CPUPhase::InterruptCheck =>
            {
                self.set_interrupt_requests(interrupt_requests);
                self.handle_interrupts(interrupt_requests);
                self.phase = CPUPhase::Fetch;
            }
        }


        /* Check memory violation. */
        let address = self.memory_buffer.address;
        let is_requesting_kernel_space =  self.memory_buffer.data_size > 0 &&
            address & 0x80000000 != 0;
        let kernel_memory_violation = is_requesting_kernel_space && !self.is_kernel_mode();

        if kernel_memory_violation
        {
            self.memory_buffer.data_size = 0; // Cancel the illegal transmission.
            let exception_code = match self.memory_buffer.store
            {
                true => ExceptionCode::IllegalAddressStore,
                false => ExceptionCode::IllegalAddressLoad,
            };

            self.execute_exception(exception_code, Some(address));
        }

        self.memory_buffer
    }

    fn handle_interrupts(&mut self, interrupt_requests: u8)
    {
        let status = &self.cp0_reg[12];

        let interrupts_enabled = (status & 0x01) == 1;
        if !interrupts_enabled
        {
            return;
        }

        let mask = ((status >> 8) & 0xFF) as u8;
        let non_masked_interrupts = interrupt_requests & mask;
        if non_masked_interrupts != 0
        {
            self.execute_exception(ExceptionCode::Interrupt, None);
        }
    }

    fn decode_and_execute(&mut self, instruction: u32)
    {
        /*
            RFE encoding
            https://people.cs.pitt.edu/~don/coe1502/current/Unit4a/Unit4a.html
         */
        if instruction == 0b010000_1_0000000000000000000_010000
        {
            self.rfe();
            return;
        }

        /*
            https://www.math.unipd.it/~sperduti/ARCHITETTURE-1/mips32.pdf
         */
        if instruction == 0b010000_1_0000000000000000000_010010
        {
            self.eret();
            return;
        }

        self.decode_cp0(instruction);
        self.decode_cp1(instruction);
        self.decode_trap_instruction(instruction);
        self.decode_int_instruction(instruction);
    }

    fn decode_cp0(&mut self, instruction: u32)
    {
        let opcode = instruction >> 26;
        let rs = ((instruction >> 21) & 0b11111) as u8;
        let rt = ((instruction >> 16) & 0b11111) as u8;
        let rd = ((instruction >> 11) & 0b11111) as u8;
        let funct = (instruction & 0b111111) as u8;

        match (opcode, rs, funct)
        {
            (16, 0, 0) => self.mfc0(rt, rd),
            (16, 4, 0) => self.mtc0(rt, rd),
            // lwc0?
            // swc0?
            _ => {},
        };
    }

    fn decode_cp1(&mut self, instruction: u32)
    {
        let opcode = instruction >> 26;
        let opcode2 = ((instruction >> 21) & 0b11111) as u8;
        let ft = ((instruction >> 16) & 0b11111) as u8;
        let early_cc = ((instruction >> 18) & 0b111) as u8;
        let after_early_cc = (instruction >> 16) & 0b11;
        let fs = ((instruction >> 11) & 0b11111) as u8;
        let fd = ((instruction >> 6) & 0b11111) as u8;

        let late_cc = ((instruction >> 8) & 0b111) as u8;
        let after_late_cc = (instruction >> 6) & 0b11;
        let last = instruction & 0b111111;

        let offset = (instruction & 0xFFFF) as u16;

        match opcode
        {
            0x39 => self.swc1(ft, opcode2, offset),
            0x31 => self.lwc1(ft, opcode2, offset),
            _ => {},
        }

        match (opcode, opcode2, fd, last)
        {
            (0x11, 0, 0, 0) => self.mfc1(ft, fs),
            (0x11, 4, 0, 0) => self.mtc1(ft, fs),
            _ => {},
        }

        match (opcode, opcode2, ft, last)
        {
            (0x11, 1, 0, 5) => self.abs_d(fd, fs),
            (0x11, 0, 0, 5) => self.abs_s(fd, fs),
            (0x11, 0x11, _, 0) => self.add_d(fd, fs, ft),
            (0x11, 0x10, _, 0) => self.add_s(fd, fs, ft),
            (0x11, 0x11, 0, 0xE) => self.ceil_w_d(fd, fs),
            (0x11, 0x10, 0, 0xE) => self.ceil_w_s(fd, fs),
            (0x11, 0x10, 0, 0x21) => self.cvt_d_s(fd, fs),
            (0x11, 0x14, 0, 0x21) => self.cvt_d_w(fd, fs),
            (0x11, 0x11, 0, 0x20) => self.cvt_s_d(fd, fs),
            (0x11, 0x14, 0, 0x20) => self.cvt_s_w(fd, fs),
            (0x11, 0x11, 0, 0x24) => self.cvt_w_d(fd, fs),
            (0x11, 0x10, 0, 0x24) => self.cvt_w_s(fd, fs),
            (0x11, 0x11, _, 3) => self.div_d(fd, fs, ft),
            (0x11, 0x10, _, 3) => self.div_s(fd, fs, ft),
            (0x11, 0x11, 0, 0xF) => self.floor_w_d(fd, fs),
            (0x11, 0x10, 0, 0xF) => self.floor_w_s(fd, fs),
            (0x11, 0x11, 0, 6) => self.mov_d(fd, fs),
            (0x11, 0x10, 0, 6) => self.mov_s(fd, fs),
            (0x11, 0x11, rt, 0x13) => self.movn_d(fd, fs, rt), // rt instead of ft
            (0x11, 0x10, rt, 0x13) => self.movn_s(fd, fs, rt), // rt instead of ft
            (0x11, 0x11, rt, 0x12) => self.movz_d(fd, fs, rt), // rt instead of ft
            (0x11, 0x10, rt, 0x12) => self.movz_s(fd, fs, rt), // rt instead of ft
            (0x11, 0x11, _, 2) => self.mul_d(fd, fs, ft),
            (0x11, 0x10, _, 2) => self.mul_s(fd, fs, ft),
            (0x11, 0x11, 0, 7) => self.neg_d(fd, fs),
            (0x11, 0x10, 0, 7) => self.neg_s(fd, fs),
            (0x11, 0x11, 0, 0xC) => self.round_w_d(fd, fs),
            (0x11, 0x10, 0, 0xC) => self.round_w_s(fd, fs),
            (0x11, 0x11, 0, 4) => self.sqrt_d(fd, fs),
            (0x11, 0x10, 0, 4) => self.sqrt_s(fd, fs),
            (0x11, 0x11, _, 1) => self.sub_d(fd, fs, ft),
            (0x11, 0x10, _, 1) => self.sub_s(fd, fs, ft),
            (0x11, 0x11, 0, 0xD) => self.trunc_d(fd, fs),
            (0x11, 0x10, 0, 0xD) => self.trunc_s(fd,fs),
            _ => {},
        }

        match (opcode, opcode2, after_late_cc, last)
        {
            (0x11, 0x11, 0, 0x32) => self.c_eq_d(late_cc, fs, ft),
            (0x11, 0x10, 0, 0x32) => self.c_eq_s(late_cc, fs, ft),
            (0x11, 0x11, 0, 0x3E) => self.c_le_d(late_cc, fs, ft),
            (0x11, 0x10, 0, 0x3E) => self.c_le_s(late_cc, fs, ft),
            (0x11, 0x11, 0, 0x3C) => self.c_lt_d(late_cc, fs, ft),
            (0x11, 0x10, 0, 0x3C) => self.c_lt_s(late_cc, fs, ft),
            _ => {},
        }

        match (opcode, opcode2, early_cc, after_early_cc, last)
        {
            (0x11, 0x11, early_cc, 0, 0x11) => self.movf_d(fd, fs, early_cc),
            (0x11, 0x10, early_cc, 0, 0x11) => self.movf_s(fd, fs, early_cc),
            (0x11, 0x11, early_cc, 1, 0x11) => self.movt_d(fd, fs, early_cc),
            (0x11, 0x10, early_cc, 1, 0x11) => self.movt_s(fd, fs, early_cc),
            _ => {},
        }
    }

    fn decode_trap_instruction(&mut self, instruction: u32)
    {
        let opcode = instruction >> 26;
        let rs = ((instruction >> 21) & 0b11111) as u8;
        let rt = ((instruction >> 16) & 0b11111) as u8;
        let imm = (instruction & 0xFFFF) as u16;

        // trap instructions
        match (opcode, rt, imm)
        {
            (0, _,  0x34) => self.teq(rs, rt),
            (1, 0xc, _) => self.teqi(rs, imm),
            (0, _, 0x36) => self.tne(rs, rt),
            (1, 0xe, _) => self.tnei(rs, imm),
            (0, _, 0x30) => self.tge(rs, rt),
            (0, _, 0x31) => self.tgeu(rs, rt),
            (1, 8, _) => self.tgei(rs, imm),
            (1, 9, _) => self.tgeiu(rs, imm),
            (0, _, 0x32) => self.tlt(rs, rt),
            (0, _, 0x33) => self.tltu(rs, rt),
            (1, 0xA, _) => self.tlti(rs, imm),
            (1, 0xB, _) => self.tltiu(rs, imm),
            _ => {},
        };
    }

    fn decode_int_instruction(&mut self, instruction: u32)
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
            (34, _) => self.lwl(rt, rs, imm),
            (35, _) => self.lw(rt, rs, imm),
            (36, _) => self.lbu(rt, rs, imm),
            (37, _) => self.lhu(rt, rs, imm),
            (38, _) => self.lwr(rt, rs, imm),
            (40, _) => self.sb(rt, rs, imm),
            (41, _) => self.sh(rt, rs, imm),
            (43, _) => self.sw(rt, rs, imm),
            _ => panic!("Bad instruction: {:X}", instruction),
        }
    }

    fn write_back(&mut self)
    {
        if let Some(_) = self.memory_buffer.partial_write
        {
            self.partial_write_back();
            return;
        }

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
        match register
        {
            0..=31 => self.write_to_reg(register, result),
            other_address =>
            {
                let cp1_address = (other_address - 32) as usize;
                let data: f32 = unsafe {mem::transmute(data)};
                self.cp1_reg[cp1_address] = data;
            },
        }
    }

    fn partial_write_back(&mut self)
    {
        let register_number = self.memory_buffer.write_back_register;
        let mut content = self.int_reg[register_number as usize];

        let (from, to) = self.memory_buffer.partial_write.unwrap();
        if from == 0 // left write
        {
            let shift = 3 - to;
            let word_part = (self.memory_buffer.data) << shift;
            let mask: u32 = !(0xFFFFFFFF << shift);

            content = (content & mask) | word_part;
        }
        else // right write
        {
            let shift = from;
            let word_part = (self.memory_buffer.data) >> shift;
            let mask: u32 = !(0xFFFFFFFF >> shift);

            content = (content & mask) | word_part;
        }

        self.write_to_reg(register_number, content);
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
        self.execute_exception(ExceptionCode::Syscall, None); // Let the OS handle it.
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
        let low = (result & 0xFFFFFFFF) as u32;

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
            self.execute_exception(ExceptionCode::Overflow, None);
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
            self.execute_exception(ExceptionCode::Overflow, None);
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
            self.execute_exception(ExceptionCode::Overflow, None);
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
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
            partial_write: None,
        }
    }

    fn lwl(&mut self, rt: u8, base: u8, offset: u16)
    {
        let address = (self.int_reg[base as usize] as i32 + (offset as i16 as i32)) as u32;
        let word_address = address - address % 4;

        let bytes_count = (4 - address % 4) as usize;
        let partial_write = Some((0, bytes_count - 1));

        self.memory_buffer = MemoryBuffer
        {
            address: word_address,
            data: 0,
            data_size: 4,
            store: false,
            write_back_register: rt,
            sign_extended: false,
            partial_write,
        }
    }

    fn lwr(&mut self, rt: u8, base: u8, offset: u16)
    {
        let address = (self.int_reg[base as usize] as i32 + (offset as i16 as i32)) as u32;
        let word_address = address - address % 4;

        let bytes_count = (address % 4 + 1) as usize;
        let partial_write = Some((4 - bytes_count, 3));

        self.memory_buffer = MemoryBuffer
        {
            address: word_address,
            data: 0,
            data_size: 4,
            store: false,
            write_back_register: rt,
            sign_extended: false,
            partial_write,
        }
    }

    fn mfc0(&mut self, rt: u8, rd: u8)
    {
        if !self.is_kernel_mode()
        {
            panic!("Bad privilege");
        }
        self.write_to_reg(rt, self.cp0_reg[rd as usize]);
    }

    fn mtc0(&mut self, rt: u8, rd: u8)
    {
        if !self.is_kernel_mode()
        {
            panic!("Bad privilege");
        }
        self.cp0_reg[rd as usize] = self.int_reg[rt as usize];
    }


    /* Trap if... instructions */
    fn teq(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] as i32 == self.int_reg[rt as usize] as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn teqi(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 == imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tne(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] != self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tnei(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 != imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tge(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] as i32 >= self.int_reg[rt as usize] as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tgeu(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] >= self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }
    fn tgei(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 != imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tgeiu(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] >= imm as i16 as i32 as u32 // Imm is sign extended?
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tlt(&mut self, rs: u8, rt: u8)
    {
        if (self.int_reg[rs as usize] as i32) < self.int_reg[rt as usize] as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tltu(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] < self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tlti(&mut self, rs: u8, imm: u16)
    {
        if (self.int_reg[rs as usize] as i32) < imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn tltiu(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] < imm as i16 as i32 as u32 // Imm is signed extended?
        {
            self.execute_exception(ExceptionCode::CalledTrap, None);
        }
    }

    fn rfe(&mut self)
    {
        if !self.is_kernel_mode()
        {
            panic!("Bad privilege");
        }

        let status = &mut self.cp0_reg[12];

        let old_previous = (*status & 0b111100) >> 2;
        *status &= !0b1111; // clear previous and current
        *status |= old_previous; // restore previous and current
    }

    fn eret(&mut self)
    {
        self.rfe();
        self.pc = self.cp0_reg[14];
    }
}

impl CPU
{
    fn branch(&mut self, imm: u16)
    {
        let offset = ((imm as i16) * 4) as i32;
        let new_pc = (self.pc as i32 + offset) as u32;
        self.pc = new_pc;
    }

    // fn set_interrupt_pending(&mut self, interrupt_number: u8)
    // {
    //     let cause = &mut self.cp0_reg[13];
    //     *cause |= 1 << (interrupt_number & 0b111 + 8);
    // }
    //
    // fn clear_interrupt_pending(&mut self, interrupt_number: u8)
    // {
    //     let cause = &mut self.cp0_reg[13];
    //     *cause &= !(1 << (interrupt_number & 0b111 + 8));
    // }

    fn set_interrupt_requests(&mut self, interrupt_requests: u8)
    {
        let cause = &mut self.cp0_reg[13];
        *cause &= !(0xFF << 8); // Clear old interrupt requests.
        *cause |= (interrupt_requests as u32) << 8; // Set new interrupt requests.
    }

    fn execute_exception(&mut self, exception_code: ExceptionCode, bad_address: Option<u32>)
    {
        if let Some(address) = bad_address
        {
            self.cp0_reg[8] = address;
        }

        /* Set exception cause */
        let cause = &mut self.cp0_reg[13];
        *cause &= !0b1111100; // clear old exception code
        *cause |= (exception_code as u32 & 0b11111) << 2; // set new exception code

        /* Set processor status */
        let status = &mut self.cp0_reg[12];
        let previous_current_status = 0b1111; // keep statuses
        *status &= !0b111111; // clear statuses
        *status |= previous_current_status << 2; // save statuses in old and previous
        // Now current status is 00 (kernel, interrupts disabled)


        self.cp0_reg[14] = self.pc; // Save return address in EPC
        self.pc = EXCEPTION_HANDLER_ADDRESS; // Jump to exception handler
    }
}

impl CPU // FP coprocessor1
{
    fn get_double_precision(&self, reg_num: u8) -> f64
    {
        if reg_num % 2 == 1
        {
            panic!("FP register not even");
        }

        let upper: u32 = unsafe {mem::transmute(self.cp1_reg[(reg_num + 1) as usize])};
        let lower: u32 = unsafe {mem::transmute(self.cp1_reg[reg_num as usize])};

        let joined: u64 = ((upper as u64) << 32) | (lower as u64);
        let result: f64 = unsafe{mem::transmute(joined)};
        result
    }

    fn write_to_double_register(&mut self, reg_num: u8, data: f64)
    {
        if reg_num % 2 == 1
        {
            panic!("FP register not even");
        }

        let bits: u64 = unsafe {mem::transmute(data)};

        let upper_bits: u32 = (bits >> 32) as u32;
        let lower_bits: u32 = (bits & 0xFFFFFFFF) as u32;

        let upper: f32 = unsafe {mem::transmute(upper_bits)};
        let lower: f32 = unsafe {mem::transmute(lower_bits)};

        self.cp1_reg[reg_num as usize] = lower;
        self.cp1_reg[(reg_num as usize) + 1] = upper;
    }

    fn mfc1(&mut self, rt: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result: u32 = unsafe {mem::transmute(op1)};
        self.write_to_reg(rt, result);
    }

    fn mtc1(&mut self, rt: u8, fs: u8)
    {
        let op1 = self.int_reg[rt as usize];
        let result: f32 = unsafe {mem::transmute(op1)};

        self.cp1_reg[fs as usize] = result;
    }

    fn lwc1(&mut self, ft: u8, base: u8, offset: u16)
    {
        let offset = offset as i16;
        let address = (self.int_reg[base as usize] as i32 + offset as i32) as u32;

        let register_number = ft + 32;

        self.memory_buffer = MemoryBuffer
        {
            address,
            data: 0,
            data_size: 4,
            store: false,
            write_back_register: register_number,
            sign_extended: false,
            partial_write: None,
        }
    }

    fn swc1(&mut self, ft: u8, base: u8, offset: u16)
    {
        let data: u32 = unsafe {mem::transmute(self.cp1_reg[ft as usize])};

        let offset = offset as i16;
        let address = (self.int_reg[base as usize] as i32 + offset as i32) as u32;

        self.memory_buffer = MemoryBuffer
        {
            address,
            data,
            data_size: 4,
            store: true,
            write_back_register: 0,
            sign_extended: false,
            partial_write: None,
        }
    }

    fn abs_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let result = op1.abs();

        self.write_to_double_register(fd, result);
    }

    fn abs_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1.abs();
        self.cp1_reg[fd as usize] = result;
    }

    fn add_d(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        let result = op1 + op2;

        self.write_to_double_register(fd, result);
    }

    fn add_s(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        let result = op1 + op2;

        self.cp1_reg[fd as usize] = result;
    }

    fn ceil_w_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let ceil_value = op1.ceil() as i32;

        let bits: f32 = unsafe {mem::transmute(ceil_value)};

        self.cp1_reg[fd as usize] = bits;
    }

    fn ceil_w_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let ceil_value = op1.ceil() as i32;

        let result: f32 = unsafe {mem::transmute(ceil_value)};
        self.cp1_reg[fd as usize] = result;
    }

    fn c_eq_d(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        if op1 == op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn c_eq_s(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        if op1 == op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn c_le_d(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        if op1 <= op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn c_le_s(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        if op1 <= op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn c_lt_d(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        if op1 < op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn c_lt_s(&mut self, cc_num: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        if op1 < op2
        {
            self.cc[cc_num as usize] = true;
        }
    }

    fn cvt_d_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1 as f64;

        self.write_to_double_register(fd, result);
    }

    fn cvt_d_w(&mut self, fd: u8, fs: u8) // convert int to double
    {
        let op1 = self.get_double_precision(fs);

        let bits: i64 = unsafe {mem::transmute(op1)};
        let result = bits as f64;

        self.write_to_double_register(fd, result);
    }

    fn cvt_s_d(&mut self, fd: u8, fs: u8) // convert double to single
    {
        let op1 = self.get_double_precision(fs);
        let result = op1 as f32;

        self.cp1_reg[fd as usize] = result;
    }

    fn cvt_s_w(&mut self, fd: u8, fs: u8) // convert int to single
    {
        let op1 = self.cp1_reg[fs as usize];
        let bits: i32 = unsafe {mem::transmute(op1)};

        let result = bits as f32;
        self.cp1_reg[fd as usize] = result;
    }

    fn cvt_w_d(&mut self, fd: u8, fs: u8) // convert double to int32
    {
        let op1 = self.get_double_precision(fs);
        let converted = op1 as i32;

        let converted_bits: f32 = unsafe {mem::transmute(converted)};

        self.cp1_reg[fd as usize] = converted_bits;
    }

    fn cvt_w_s(&mut self, fd: u8, fs: u8) // convert single to int32
    {
        let op1 = self.cp1_reg[fs as usize];
        let converted = op1 as i32;

        let converted_bits: f32 = unsafe {mem::transmute(converted)};

        self.cp1_reg[fd as usize] = converted_bits;
    }

    fn div_d(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        let result = op1 / op2;

        self.write_to_double_register(fd, result);
    }

    fn div_s(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        let result = op1 / op2;

        self.cp1_reg[fd as usize] = result;
    }

    fn floor_w_d(&mut self, fd: u8, fs: u8) // floor of f64 as i32
    {
        let op1 = self.get_double_precision(fs);
        let result = op1.ceil() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }

    fn floor_w_s(&mut self, fd: u8, fs: u8) // floor of f32 as i32
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1.ceil() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }

    fn mov_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        self.write_to_double_register(fd, op1);
    }

    fn mov_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        self.cp1_reg[fd as usize] = op1;
    }

    fn movf_d(&mut self, fd: u8, fs: u8, cc_num: u8)
    {
        if self.cc[cc_num as usize] == false
        {
            self.mov_d(fd, fs);
        }
    }

    fn movf_s(&mut self, fd: u8, fs: u8, cc_num: u8)
    {
        if self.cc[cc_num as usize] == false
        {
            self.mov_s(fd, fs);
        }
    }

    fn movt_d(&mut self, fd: u8, fs: u8, cc_num: u8)
    {
        if self.cc[cc_num as usize] == true
        {
            self.mov_d(fd, fs);
        }
    }

    fn movt_s(&mut self, fd: u8, fs: u8, cc_num: u8)
    {
        if self.cc[cc_num as usize] == true
        {
            self.mov_s(fd, fs);
        }
    }

    fn movn_d(&mut self, fd: u8, fs: u8, rt: u8)
    {
        if self.cp0_reg[rt as usize] != 0
        {
            self.mov_d(fd, fs);
        }
    }

    fn movn_s(&mut self, fd: u8, fs: u8, rt: u8)
    {
        if self.cp0_reg[rt as usize] != 0
        {
            self.mov_s(fd, fs);
        }
    }

    fn movz_d(&mut self, fd: u8, fs: u8, rt: u8)
    {
        if self.cp0_reg[rt as usize] == 0
        {
            self.mov_d(fd, fs);
        }
    }

    fn movz_s(&mut self, fd: u8, fs: u8, rt: u8)
    {
        if self.cp0_reg[rt as usize] == 0
        {
            self.mov_s(fd, fs);
        }
    }

    fn mul_d(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        let result = op1 * op2;

        self.write_to_double_register(fd, result);
    }

    fn mul_s(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        let result = op1 * op2;

        self.cp1_reg[fd as usize] = result;
    }

    fn neg_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let result = -op1;

        self.write_to_double_register(fd, result);
    }

    fn neg_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = -op1;

        self.cp1_reg[fd as usize] = result;
    }

    fn round_w_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let result = op1.round() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }

    fn round_w_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1.round() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }

    fn sqrt_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let result = op1.sqrt();
        self.write_to_double_register(fd, result);
    }

    fn sqrt_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1.sqrt();
        self.cp1_reg[fd as usize] = result;
    }

    fn sub_d(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.get_double_precision(fs);
        let op2 = self.get_double_precision(ft);

        let result = op1 - op2;

        self.write_to_double_register(fd, result);
    }

    fn sub_s(&mut self, fd: u8, fs: u8, ft: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let op2 = self.cp1_reg[ft as usize];

        let result = op1 - op2;
        self.cp1_reg[fd as usize] = result;
    }

    fn trunc_d(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.get_double_precision(fs);
        let result = op1.trunc() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }

    fn trunc_s(&mut self, fd: u8, fs: u8)
    {
        let op1 = self.cp1_reg[fs as usize];
        let result = op1.trunc() as i32;

        let bits: f32 = unsafe {mem::transmute(result)};
        self.cp1_reg[fd as usize] = bits;
    }
}
