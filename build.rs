use std::io;
#[cfg(windows)]
use winres::WindowsResource;

fn main() -> io::Result<()> {
    #[cfg(windows)]
    {
        WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()?;
    }
    cc::Build::new()
        .file("src/lz4/lz4.c")
        .compile("alz4");
    Ok(())
}
