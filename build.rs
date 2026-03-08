use std::io;

fn main() -> io::Result<()> {
    built::write_built_file().expect("Failed to acquire build-time information");

    #[cfg(windows)]
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some(){
        winres::WindowsResource::new()
            .set_icon("assets/favicon.ico")
            .compile()?;
    }
    Ok(())
}
