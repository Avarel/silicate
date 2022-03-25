mod canvas;
mod error;
mod gpu;
mod ns_archive;
mod silica;

use silica::ProcreateFile;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }
    let procreate = ProcreateFile::open(&args[1])?;

    // let mut composite = Rgba8Canvas::new(
    //     procreate.size.width,
    //     procreate.size.height,
    // );
    //RgbaImage::new(procreate.size.width, procreate.size.height);
    // render(&mut composite, &mut procreate.layers);
    // canvas::adapter::adapt(composite).save("./out/final.png")?;

    gpu::gpu_render(
        procreate.size.width,
        procreate.size.height,
        if procreate.background_hidden {
            None
        } else {
            Some(procreate.background_color)
        },
        &procreate.layers,
    );

    canvas::adapter::adapt(procreate.composite.image.unwrap()).save("./out/reference.png")?;
    // gpu::gpu_render(&procreate.composite.image.unwrap());
    Ok(())
}
