fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/inzel.ico");
        res.set("ProductName", "Inzel");
        res.set("FileDescription", "Inzel - voice typing & translator");
        res.set("CompanyName", "Debugger Mohon");
        res.set("LegalCopyright", "MIT License");
        res.compile().expect("failed to embed icon/version resource");
    }
}
