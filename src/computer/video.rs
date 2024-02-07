use minifb::{Window, WindowOptions};
use crate::computer::memory::Memory;

pub(super) struct Video
{
    vram_start: usize,
    vram_size: usize,
    window: Window,
}

impl Video
{
    pub(super) fn new(width: usize, height: usize, vram_start: usize) -> Video
    {
        Video
        {
            vram_start,
            vram_size: width * height * 3,
            window: Window::new("super emulator kurwo", width, height, WindowOptions::default())
                .unwrap(),
        }
    }

    pub(super) fn display(&mut self, memory: &Memory)
    {
        let start = self.vram_start;
        let end = self.vram_start + self.vram_size;
        let mut buffer = vec![0; self.vram_size];

        for addr in start..end
        {
            let (r, g, b) = (
                memory.read_data(addr as u32, 1),
                memory.read_data((addr + 1) as u32, 1),
                memory.read_data((addr + 2) as u32, 1)
            );

            let color_val = (r << 16) | (g << 8) | b;
            buffer.push(color_val);
        }

        let (width, height) = self.window.get_size();
        self.window.update_with_buffer(&buffer, width, height)
            .unwrap();
    }
}