use crate::computer::cpu::CPU;
use crate::computer::memory::Memory;

pub mod cpu;
mod memory;

pub struct Computer
{
    cpu: CPU,
    ram: Memory,
}

impl Computer
{
    pub fn new(memory_size: usize) -> Computer
    {
        Computer
        {
            cpu: CPU::new(),
            ram: Memory::new(memory_size),
        }
    }

    fn step(&mut self)
    {
        // fetch
        let mem_request = self.cpu.tick(0);
        let pc = mem_request.address;
        let instruction = self.ram.read_data(pc, 4);

        // execute
        let mem_request = self.cpu.tick(instruction);

        // check for memory request
        match (mem_request.data_size, mem_request.store, mem_request.address)
        {
            (0, _, _) => self.cpu.tick(0), // no cpu ram transmission
            (size, false, addr) => self.cpu.tick(self.ram.read_data(addr, size)),
            (size, true, addr) =>
            {
                let data = mem_request.data;
                self.ram.write_data(addr, data, size);
                self.cpu.tick(0)
            }
        };
    }

    pub fn run(&mut self)
    {
        loop
        {
            self.step();
        }
    }
}
