use crate::computer::Computer;

mod computer;

fn main()
{
    println!("Hello, world!");

    let mut computer = Computer::new(u32::MAX as usize);
    computer.run();
}
