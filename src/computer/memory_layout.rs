use std::ops::Range;

pub(super) struct MemoryLayout
{
    pub(super) program: Range<u32>,
    pub(super) video_ram: Range<u32>,
    pub(super) data: Range<u32>,
}