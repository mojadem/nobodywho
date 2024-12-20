{ fetchurl, rustPlatform, libclang, llvmPackages_12, stdenv, lib, cmake, vulkan-headers, vulkan-loader, vulkan-tools, shaderc, mesa }:


rustPlatform.buildRustPackage {
  pname = "nobody";
  version = "0.0.0";
  src = ./.;
  nativeBuildInputs = [ llvmPackages_12.bintools cmake vulkan-headers vulkan-loader shaderc vulkan-tools mesa.drivers ];
  buildInputs = [ vulkan-loader vulkan-headers shaderc vulkan-tools mesa.drivers ];
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "gdextension-api-0.2.0" = "sha256-kIkOMwbO63pnmwG3nyM0gMtWhCKSMqz6fmd2nQ22wHg=";
      "godot-0.1.3" = "sha256-T4HVbQ707obITx2dYAO8UYDM9Dvk6LMn6G3Ue8M1KqU=";
    };
  };
  env.LIBCLANG_PATH = "${libclang.lib}/lib/libclang.so";
  env.TEST_MODEL = fetchurl {
    name = "gemma-2-2b-it-Q5_K_M.gguf";
    url = "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q5_K_M.gguf";
    sha256 = "1njh254wpsg2j4wi686zabg63n42fmkgdmf9v3cl1zbydybdardy";
  };

  # See: https://hoverbear.org/blog/rust-bindgen-in-nix/
  preBuild = ''
    # From: https://github.com/NixOS/nixpkgs/blob/1fab95f5190d087e66a3502481e34e15d62090aa/pkgs/applications/networking/browsers/firefox/common.nix#L247-L253
    # Set C flags for Rust's bindgen program. Unlike ordinary C
    # compilation, bindgen does not invoke $CC directly. Instead it
    # uses LLVM's libclang. To make sure all necessary flags are
    # included we need to look in a few places.
    export BINDGEN_EXTRA_CLANG_ARGS="$(< ${stdenv.cc}/nix-support/libc-crt1-cflags) \
      $(< ${stdenv.cc}/nix-support/libc-cflags) \
      $(< ${stdenv.cc}/nix-support/cc-cflags) \
      $(< ${stdenv.cc}/nix-support/libcxx-cxxflags) \
      ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
      ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
    "
  '';

  checkPhase = ''
    cargo test -- --test-threads=1 --nocapture
  '';
  doCheck = true;
}
