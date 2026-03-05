use std::io;

fn main() -> io::Result<()> {
    #[cfg(windows)]
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some(){
        winres::WindowsResource::new()
            .set_icon("assets/favicon.ico")
            .compile()?;
    }
    Ok(())
}
