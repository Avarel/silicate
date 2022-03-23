mod composite;
mod error;
mod ns_archive;
mod silica;
mod canvas;

use image::{Rgba, RgbaImage};
use silica::{ProcreateFile, SilicaGroup, SilicaHierarchy};
use std::error::Error;

type Rgba8 = Rgba<u8>;

fn main() -> Result<(), Box<dyn Error>> {
    let mut procreate = ProcreateFile::open("./Gilvana.procreate")?;

    let mut composite = RgbaImage::new(procreate.size.width, procreate.size.height);
    render(&mut composite, &mut procreate.layers);
    composite.save("./out/final.png")?;

    procreate.composite.image.unwrap().save("./out/reference.png")?;
    Ok(())
}

fn render(composite: &mut RgbaImage, layers: &SilicaGroup) {
    let mut mask: Option<RgbaImage> = None;

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
                        composite::layer_clip(&mut layer_image, &mask, layer.opacity);
                    }
                }

                composite::layer_blend(
                    composite,
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
