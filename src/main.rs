use crate::computer::Computer;

mod computer;

fn main()
{
    println!("Hello, world!");

    let mut computer = Computer::new(1024 * 1024 * 1024, 800, 600);
    computer.run();
}
