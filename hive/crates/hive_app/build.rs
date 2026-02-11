fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("hive_bee.ico");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to embed icon: {e}");
        }
    }
}
