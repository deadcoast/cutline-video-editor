fn main() {
    println!("cargo:rerun-if-changed=assets/cutline.ico");
    println!("cargo:rerun-if-changed=assets/windows.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/cutline.ico");
        resource.set_manifest_file("assets/windows.manifest");
        resource.set("ProductName", "Cutline");
        resource.set("FileDescription", "Cutline — view, trim and crop video");
        resource.set("OriginalFilename", "Cutline.exe");
        resource.compile().expect("Windows resources must compile");
    }
}
