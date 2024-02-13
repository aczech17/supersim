use crate::computer::cpu::CPU;
use crate::computer::memory::Memory;
use crate::memory_layout::MemoryLayout;
use crate::computer::video::Video;

pub mod cpu;
mod memory;
mod video;

pub struct Computer
{
    cpu: CPU,
    ram: Memory,
    video: Video,
}

impl Computer
{
    pub fn new(memory_size: usize, display_width: usize, display_height: usize,
        memory_layout: MemoryLayout) -> Computer
    {
        let mut ram = Memory::new(memory_size);

        let program_start = memory_layout.program.start;
        let loop_instruction: u32 = 0b0000_1000_0000_0000_0000_0000_0000_0000;
        ram.write_data(program_start, loop_instruction, 4);

        let vram_start = memory_layout.video_ram.start;
        //
        // println!("filling vram");
        // for address in (vram_start..memory_size).step_by(4)
        // {
        //     ram.write_data(address as u32, 0xFF_00_00_FF, 4);
        // }
        // println!("vram filled");

        Computer
        {
            cpu: CPU::new(),
            ram,
            video: Video::new(display_width, display_height, vram_start),
        }
    }

    fn cpu_step(&mut self, interrupt_requests: u8)
    {
        // FETCH
        let mem_request = self.cpu.tick(0, interrupt_requests);
        let pc = mem_request.address;
        let instruction = self.ram.read_data(pc, 4);

        // EXECUTE
        let mem_request = self.cpu.tick(instruction, interrupt_requests);

        // check for memory request
        match (mem_request.data_size, mem_request.store, mem_request.address)
        {
            (0, _, _) => self.cpu.tick(0, interrupt_requests), // no cpu ram transmission
            (size, false, addr) => // load from RAM
            {
                let data = self.ram.read_data(addr, size);
                self.cpu.tick(data, interrupt_requests)
            },
            (size, true, addr) => // write to RAM
            {
                let data = mem_request.data;
                self.ram.write_data(addr, data, size);
                self.cpu.tick(0, interrupt_requests)
            }
        };

        // Send interrupt requests.
        self.cpu.tick(0, interrupt_requests);
    }

    pub fn run(&mut self)
    {
        let interrupt_requests = 0;
        loop
        {
            self.cpu_step(interrupt_requests);
            self.video.display(&self.ram);
        }
    }
}
