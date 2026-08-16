fn main() {
    let libtorrent = find_libtorrent();

    // Build the cxx bridge
    let mut build = cxx_build::bridge("src/lib.rs");

    // Add libtorrent include paths
    for path in &libtorrent.include_paths {
        build.include(path);
    }

    for (name, value) in &libtorrent.defines {
        build.define(name, value.as_deref());
    }

    // Add our C++ wrapper
    build
        .file("cpp/wrapper.cpp")
        .file("cpp/memory_storage.cpp")
        .std("c++17")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-maybe-uninitialized");

    if libtorrent.using_vcpkg {
        // Ensure static linking definitions are present for vcpkg builds
        // These are critical for both Windows and Linux to avoid linker errors
        build.define("TORRENT_LINKING_STATIC", None);
        build.define("BOOST_ASIO_STATIC_LINK", None);

        // The Windows vcpkg build supplies Boost.Asio's separately compiled
        // implementation. Linux vcpkg uses header-only Asio, so defining this
        // there would leave the wrapper with unresolved Asio symbols.
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            build.define("BOOST_ASIO_SEPARATE_COMPILATION", None);
        }
    }

    build.define("TORRENT_USE_OPENSSL", None);

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        build.flag_if_supported("/std:c++17");
        build.flag("/Zc:__cplusplus"); // Force correct C++ version
        build.flag_if_supported("/EHsc"); // Enable C++ exceptions
    }

    // Compile
    build.compile("libtorrent_wrapper");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cpp/wrapper.cpp");
    println!("cargo:rerun-if-changed=cpp/wrapper.h");
    println!("cargo:rerun-if-changed=cpp/memory_storage.hpp");
    println!("cargo:rerun-if-changed=cpp/memory_storage.cpp");
}

struct LibtorrentConfig {
    include_paths: Vec<std::path::PathBuf>,
    using_vcpkg: bool,
    defines: Vec<(String, Option<String>)>,
}

fn find_libtorrent() -> LibtorrentConfig {
    let mut errors = String::new();

    // Try pkg-config first
    let mut pkg_config = pkg_config::Config::new();
    pkg_config.atleast_version("2.1.1");
    if std::env::var_os("LIBTORRENT_STATIC").is_some() {
        pkg_config.statik(true);
    }

    match pkg_config.probe("libtorrent-rasterbar") {
        Ok(lib) => {
            // Check if this is a vcpkg-installed library by looking at the path
            let is_vcpkg = lib
                .include_paths
                .iter()
                .any(|p| p.to_string_lossy().contains("vcpkg_installed"));
            return LibtorrentConfig {
                include_paths: lib.include_paths,
                using_vcpkg: is_vcpkg,
                defines: lib.defines.into_iter().collect(),
            };
        }
        Err(e) => {
            errors.push_str(&format!("pkg-config: {}\n", e));
        }
    }

    // Configure vcpkg crate
    let triplet_env = std::env::var("VCPKGRS_TRIPLET").ok();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    let static_crt = target_features
        .split(',')
        .any(|feature| feature == "crt-static");
    let default_triplet = if target_os == "windows" && static_crt {
        "x64-windows-static"
    } else if target_os == "windows" {
        "x64-windows-static-md"
    } else {
        "x64-linux"
    };
    let triplet_ref = triplet_env.as_deref().unwrap_or(default_triplet);

    unsafe {
        std::env::set_var("VCPKGRS_DYNAMIC", "0");
        std::env::set_var("VCPKGRS_TRIPLET", triplet_ref);
    }

    // Fallback to vcpkg crate
    let packages = ["libtorrent", "libtorrent-rasterbar"];
    for package in packages {
        match vcpkg::Config::new()
            .emit_includes(true)
            .find_package(package)
        {
            Ok(lib) => {
                return LibtorrentConfig {
                    include_paths: lib.include_paths,
                    using_vcpkg: true,
                    defines: Vec::new(),
                };
            }
            Err(e) => {
                errors.push_str(&format!("vcpkg ({}): {}\n", package, e));
            }
        }
    }

    // Last resort: Manual discovery if VCPKG_INSTALLED_DIR is set (common in CI)
    if let Ok(installed_dir) = std::env::var("VCPKG_INSTALLED_DIR") {
        let include_path = std::path::PathBuf::from(installed_dir)
            .join(triplet_ref)
            .join("include");

        if include_path.exists() {
            // If we found it manually, we also need to tell cargo where the libs are
            let lib_path = include_path.parent().unwrap().join("lib");
            if lib_path.exists() {
                println!("cargo:rustc-link-search=native={}", lib_path.display());
                println!("cargo:rustc-link-lib=static=torrent-rasterbar");
                // On Windows, we also need to link with some system libs for boost/vcpkg
                if target_os == "windows" {
                    println!("cargo:rustc-link-lib=static=libssl");
                    println!("cargo:rustc-link-lib=static=libcrypto");
                    println!("cargo:rustc-link-lib=crypt32");
                    println!("cargo:rustc-link-lib=user32");
                    println!("cargo:rustc-link-lib=gdi32");
                    println!("cargo:rustc-link-lib=advapi32");
                    println!("cargo:rustc-link-lib=iphlpapi");
                    println!("cargo:rustc-link-lib=dbghelp");
                }
                return LibtorrentConfig {
                    include_paths: vec![include_path],
                    using_vcpkg: true,
                    defines: Vec::new(),
                };
            }
        }
    }

    panic!("Could not find libtorrent.\nErrors:\n{}", errors);
}
