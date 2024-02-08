use minifb::{Window, WindowOptions};
use crate::computer::memory::Memory;

pub(super) struct Video
{
    vram_start: u32,
    vram_size: u32,
    window: Window,
}

impl Video
{
    pub(super) fn new(width: usize, height: usize, vram_start: u32) -> Video
    {
        Video
        {
            vram_start,
            vram_size: (width * height * 4) as u32,
            window: Window::new("super emulator kurwo", width, height, WindowOptions::default())
                .unwrap(),
        }
    }

    pub(super) fn display(&mut self, memory: &Memory)
    {
        let start = self.vram_start;
        let end = self.vram_start + self.vram_size;
        let mut buffer = Vec::new();

        for addr in (start..end).step_by(4)
        {
            let pixel = memory.read_data(addr as u32, 4);
            buffer.push(pixel);
        }

        let (width, height) = self.window.get_size();
        self.window.update_with_buffer(&buffer, width, height)
            .unwrap();
    }
}