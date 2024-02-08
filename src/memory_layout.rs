use std::ops::Range;

pub(crate) struct MemoryLayout
{
    pub(crate) program: Range<u32>,
    pub(crate) video_ram: Range<u32>,
    pub(crate) data: Range<u32>,
}