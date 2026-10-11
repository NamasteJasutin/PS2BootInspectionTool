//! PSX Studio: the PSX Boot Inspection Tool's Studio on its own — a music maker over every
//! instrument in the user's PS1/PS2 BIOS files and the banks extracted from their own discs.

fn main() -> eframe::Result { psxbootinspect::main(psxbootinspect::Edition::Studio) }
