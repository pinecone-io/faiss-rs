#[cfg(feature = "static")]
use std::env;

#[cfg(feature = "intel_mkl")]
extern crate intel_mkl_tool;

#[cfg(feature = "intel_mkl")]
use intel_mkl_tool::*;

fn main() {
    #[cfg(feature = "static")]
    static_link_faiss();
    #[cfg(not(feature = "static"))]
    println!("cargo:rustc-link-lib=faiss_c");
}

#[cfg(feature = "static")]
fn static_link_faiss() {
    let target = env::var("TARGET").expect("TARGET not set");

    let mut cfg = cmake::Config::new("faiss");
    cfg.define("FAISS_ENABLE_C_API", "ON")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define(
            "FAISS_ENABLE_GPU",
            if cfg!(feature = "gpu") { "ON" } else { "OFF" },
        )
        .define("FAISS_ENABLE_PYTHON", "OFF")
        .define("BUILD_TESTING", "OFF")
        .very_verbose(true);

    if target.contains("apple") {
        cfg.define("OpenMP_ROOT", "/opt/homebrew/opt/libomp");
    }

    #[cfg(feature = "intel_mkl")]
    {
        let mkl_lib = Library::new(Config {
            link: LinkType::Dynamic,
            index_size: DataModel::ILP64,
            parallel: Threading::OpenMP,
        })
        .expect("Intel MKL library not found");

        cfg.define("MKL_LIBRARIES", mkl_lib.library_dir.join("libmkl_rt.so"));
        cfg.define("BLA_VENDOR", "Intel10_64_dyn");
    }

    if cfg!(feature = "avx512") {
        cfg.define("FAISS_OPT_LEVEL", "avx512");
    }

    let dst = cfg.build();
    let faiss_location = dst.join("lib");
    let faiss_c_location = dst.join("build/c_api");
    println!(
        "cargo:rustc-link-search=native={}",
        faiss_location.display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        faiss_c_location.display()
    );
    println!("cargo:rustc-link-lib=static=faiss_c");
    println!("cargo:rustc-link-lib=static=faiss");
    link_cxx();

    if !target.contains("msvc") && !target.contains("apple") {
        println!("cargo:rustc-link-lib=gomp");
    } else {
        println!("cargo:rustc-link-lib=omp");
    }
    if target.contains("apple") {
        println!("cargo::rustc-link-search=/opt/homebrew/opt/libomp/lib");
    }
    if cfg!(feature = "intel_mkl") {
        println!("cargo:rustc-link-lib=mkl_rt");
    } else {
        println!("cargo:rustc-link-lib=blas");
        println!("cargo:rustc-link-lib=lapack");
    }
    if cfg!(feature = "gpu") {
        let cuda_path = cuda_lib_path();
        println!("cargo:rustc-link-search=native={}/lib64", cuda_path);
        println!("cargo:rustc-link-lib=cudart");
        println!("cargo:rustc-link-lib=cublas");
    }
}

#[cfg(feature = "static")]
fn link_cxx() {
    let cxx = match std::env::var("CXXSTDLIB") {
        Ok(s) if s.is_empty() => None,
        Ok(s) => Some(s),
        Err(_) => {
            let target = std::env::var("TARGET").unwrap();
            if target.contains("msvc") {
                None
            } else if target.contains("apple")
                | target.contains("freebsd")
                | target.contains("openbsd")
            {
                Some("c++".to_string())
            } else {
                Some("stdc++".to_string())
            }
        }
    };
    if let Some(cxx) = cxx {
        println!("cargo:rustc-link-lib={}", cxx);
    }
}

#[cfg(feature = "static")]
fn cuda_lib_path() -> String {
    // look for CUDA_PATH in environment,
    // then CUDA_LIB_PATH,
    // then CUDA_INCLUDE_PATH
    if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
        return cuda_path;
    }
    if let Ok(cuda_lib_path) = std::env::var("CUDA_LIB_PATH") {
        return cuda_lib_path;
    }
    if let Ok(cuda_include_path) = std::env::var("CUDA_INCLUDE_PATH") {
        return cuda_include_path;
    }

    panic!(
        "Could not find CUDA: environment variables `CUDA_PATH`, `CUDA_LIB_PATH`, or `CUDA_INCLUDE_PATH` must be set"
    );
}
