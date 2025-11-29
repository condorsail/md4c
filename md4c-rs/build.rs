use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let md4c_src = PathBuf::from(&manifest_dir).join("..").join("src");

    // Build md4c core library
    let mut build = cc::Build::new();
    build
        .file(md4c_src.join("md4c.c"))
        .include(&md4c_src)
        .define("MD4C_USE_UTF8", None)
        .warnings(false)
        .opt_level(3);

    // Add HTML renderer if feature is enabled
    #[cfg(feature = "html")]
    {
        build.file(md4c_src.join("md4c-html.c"));
        build.file(md4c_src.join("entity.c"));
    }

    build.compile("md4c");

    // Tell cargo to invalidate the built crate whenever the C sources change
    println!("cargo:rerun-if-changed={}", md4c_src.join("md4c.c").display());
    println!("cargo:rerun-if-changed={}", md4c_src.join("md4c.h").display());
    println!("cargo:rerun-if-changed={}", md4c_src.join("md4c-html.c").display());
    println!("cargo:rerun-if-changed={}", md4c_src.join("md4c-html.h").display());
    println!("cargo:rerun-if-changed={}", md4c_src.join("entity.c").display());
    println!("cargo:rerun-if-changed={}", md4c_src.join("entity.h").display());
}
