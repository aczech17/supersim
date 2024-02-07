pub(super) struct Memory
{
    data: Vec<u8>,
}

impl Memory
{
    pub(super) fn new(size: usize) -> Memory
    {
        Memory
        {
            data: vec![0; size],
        }
    }
    fn read_byte(&self, address: usize) -> u32
    {
        self.data[address] as u32
    }

    fn read_halfword(&self, address: usize) -> u32
    {
        let (b1, b2) = (self.data[address], self.data[address + 1]);
        (((b1 as u16) << 8) | (b2 as u16)) as u32
    }

    fn read_word(&self, address: usize) -> u32
    {
        let (b1, b2, b3, b4) = (self.data[address], self.data[address + 1],
                                self.data[address + 2], self.data[address + 3]);

        ((b1 as u32) << 24) |
            ((b2 as u32) << 16) |
            ((b3 as u32) << 8) |
            (b4 as u32)
    }

    pub(super) fn read_data(&self, address: u32, size: u8) -> u32
    {
        let size = size as usize;
        match size
        {
            1 => self.read_byte(address as usize),
            2 => self.read_halfword(address as usize),
            4 => self.read_word(address as usize),
            _ => panic!("Bad data size"),
        }
    }

    fn write_byte(&mut self, address: usize, data: u32)
    {
        self.data[address] = data as u8;
    }

    fn write_halfword(&mut self, address: usize, data: u32)
    {
        let data = data as u16;
        self.data[address] = ((data >> 8) & 0xFF) as u8;
        self.data[address + 1] = (data & 0xFF) as u8;
    }

    fn write_word(&mut self, address: usize, data: u32)
    {
        let bytes: [u8; 4] = u32::to_be_bytes(data);
        for i in 0..4
        {
            self.data[address + i] = bytes[i];
        }
    }

    pub(super) fn write_data(&mut self, address: u32, data: u32, size: u8)
    {
        let size = size as usize;
        match size
        {
            1 => self.write_byte(address as usize, data),
            2 => self.write_halfword(address as usize, data),
            4 => self.write_word(address as usize, data),
            _ => panic!("Bad data size"),
        }
    }
}




