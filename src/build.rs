use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rustc-link-lib=mkl_rt");
    Ok(())
}
