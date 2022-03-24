mod canvas;
mod composite;
mod error;
mod ns_archive;
mod silica;
mod gpu;

use canvas::Rgba8Canvas;
use silica::{ProcreateFile, SilicaGroup, SilicaHierarchy};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut procreate = ProcreateFile::open("./Gilvana.procreate")?;

    let mut composite = Rgba8Canvas::new(
        procreate.size.width,
        procreate.size.height,
    );
    //RgbaImage::new(procreate.size.width, procreate.size.height);
    // render(&mut composite, &mut procreate.layers);
    // canvas::adapter::adapt(composite).save("./out/final.png")?;

    gpu::gpu_render(procreate.size.width, procreate.size.height, &procreate.layers);

    // canvas::adapter::adapt(procreate.composite.image.unwrap()).save("./out/reference.png")?;
    // gpu::gpu_render(&procreate.composite.image.unwrap());
    Ok(())
}

fn render(composite: &mut Rgba8Canvas, layers: &SilicaGroup) {
    let mut mask: Option<Rgba8Canvas> = None;

    for layer in layers.children.iter().rev() {
        match layer {
            SilicaHierarchy::Group(group) => {
                if group.hidden {
                    eprintln!("Hidden group {:?}", group.name);
                    continue;
                }
                eprintln!("Into group {}", group.name);
                render(composite, group);
                eprintln!("Finished group {}", group.name);
            }
            SilicaHierarchy::Layer(layer) => {
                if layer.hidden {
                    eprintln!("Hidden layer {:?}", layer.name);
                    continue;
                }

                let mut layer_image = layer.image.clone().unwrap();

                if layer.clipped {
                    if let Some(mask) = &mask {
                        layer_image.layer_clip(&mask, layer.opacity);
                    }
                }

                composite.layer_blend(
                    &layer_image,
                    layer.opacity,
                    match layer.blend {
                        1 => composite::multiply,
                        2 => composite::screen,
                        11 => composite::overlay,
                        0 | _ => composite::normal,
                    },
                );

                if !layer.clipped {
                    mask = Some(layer_image);
                }

                eprintln!("Finished layer {:?}: {}", layer.name, layer.blend);
            }
        }
    }
}