mod canvas;
mod error;
mod ns_archive;
mod silica;
mod gpu;

use silica::ProcreateFile;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let procreate = ProcreateFile::open("./Gilvana.procreate")?;

    // let mut composite = Rgba8Canvas::new(
    //     procreate.size.width,
    //     procreate.size.height,
    // );
    //RgbaImage::new(procreate.size.width, procreate.size.height);
    // render(&mut composite, &mut procreate.layers);
    // canvas::adapter::adapt(composite).save("./out/final.png")?;

    gpu::gpu_render(procreate.size.width, procreate.size.height, &procreate.layers);

    canvas::adapter::adapt(procreate.composite.image.unwrap()).save("./out/reference.png")?;
    // gpu::gpu_render(&procreate.composite.image.unwrap());
    Ok(())
}