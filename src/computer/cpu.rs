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
                if interrupt_requests != 0
                {
                    self.execute_exception(ExceptionCode::Interrupt); // Let the OS handle it.
                }
                self.phase = CPUPhase::Fetch;
            }
        }


        /* Check memory violation. */
        let is_requesting_kernel_space =  self.memory_buffer.data_size > 0 &&
            self.memory_buffer.address & 0x80000000 != 0;
        let kernel_memory_violation = is_requesting_kernel_space && !self.is_kernel_mode();

        if kernel_memory_violation
        {
            self.memory_buffer.data_size = 0; // Cancel the illegal transmission.
            let exception_code = match self.memory_buffer.store
            {
                true => ExceptionCode::IllegalAddressStore,
                false => ExceptionCode::IllegalAddressLoad,
            };

            self.execute_exception(exception_code);
        }

        self.memory_buffer
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

        let opcode = instruction >> 26;
        let rs = ((instruction >> 21) & 0b11111) as u8;
        let rt = ((instruction >> 16) & 0b11111) as u8;
        let rd = ((instruction >> 11) & 0b11111) as u8;
        let shamt = ((instruction >> 6) & 0b11111) as u8;
        let funct = (instruction & 0b111111) as u8;
        let imm = (instruction & 0xFFFF) as u16;
        let address = instruction & 0x3FFFFFF;

        // coprocessor 0
        match (opcode, rs, funct)
        {
            (16, 0, 0) => self.mfc0(rt, rd),
            (16, 4, 0) => self.mtc0(rt, rd),
            // lwc0?
            // swc0?
            _ => {},
        };

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

        // integer registers
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
        self.execute_exception(ExceptionCode::Syscall); // Let the OS handle it.
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
            self.execute_exception(ExceptionCode::Overflow);
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
            self.execute_exception(ExceptionCode::Overflow);
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
            self.execute_exception(ExceptionCode::Overflow);
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
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn teqi(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 == imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tne(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] != self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tnei(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 != imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tge(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] as i32 >= self.int_reg[rt as usize] as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tgeu(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] >= self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }
    fn tgei(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] as i32 != imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tgeiu(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] >= imm as i16 as i32 as u32 // Imm is sign extended?
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tlt(&mut self, rs: u8, rt: u8)
    {
        if (self.int_reg[rs as usize] as i32) < self.int_reg[rt as usize] as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tltu(&mut self, rs: u8, rt: u8)
    {
        if self.int_reg[rs as usize] < self.int_reg[rt as usize]
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tlti(&mut self, rs: u8, imm: u16)
    {
        if (self.int_reg[rs as usize] as i32) < imm as i16 as i32
        {
            self.execute_exception(ExceptionCode::CalledTrap);
        }
    }

    fn tltiu(&mut self, rs: u8, imm: u16)
    {
        if self.int_reg[rs as usize] < imm as i16 as i32 as u32 // Imm is signed extended?
        {
            self.execute_exception(ExceptionCode::CalledTrap);
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

    fn execute_exception(&mut self, exception_code: ExceptionCode)
    {
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
        let ceil_value = op1.ceil() as i64;

        let bits: f64 = unsafe {mem::transmute(ceil_value)};

        self.write_to_double_register(fd, bits);
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
}

