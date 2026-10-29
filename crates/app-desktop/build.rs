//! On Windows: put the app icon (packaging/windows/daw388.ico) and the version into the exe.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/daw388.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../packaging/windows/daw388.ico");
        resource.set("ProductName", "DAW 388");
        resource.set("FileDescription", "DAW 388");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=no icon in the exe: {e}");
        }
    }
}
