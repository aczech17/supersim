use crate::computer::Computer;
use crate::memory_layout::MemoryLayout;

mod computer;
mod memory_layout;

fn main()
{
    const MEMORY_SIZE: u32 = 3 * 1024 * 1024;
    const SCREEN_WIDTH: u32 = 800;
    const SCREEN_HEIGHT: u32 = 600;

    let memory_layout = MemoryLayout
    {
        program: 0..4,
        video_ram: 4..(4 * SCREEN_WIDTH * SCREEN_HEIGHT),
        data: 4 * SCREEN_WIDTH * SCREEN_HEIGHT..MEMORY_SIZE,
    };

    let mut computer = Computer::new(1024 * 1024 * 32, 800,
                                     600, memory_layout);
    computer.run();
}
