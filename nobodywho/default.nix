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
      "gdextension-api-0.2.1" = "sha256-YkMbzObJGnmQa1XGT4ApRrfqAeOz7CktJrhYks8z0RY=";
      "godot-0.2.0" = "sha256-jv0SsEiVrjCcOCVgc43gsTRk6HTImyRkRpZQRQD6rwc=";
      "llama-cpp-2-0.1.86" = "sha256-Fe8WPO1NAISGGDkX5UWM8ubekYbnnAwEcKf0De5x9AQ=";
    };
  };
  env.LIBCLANG_PATH = "${libclang.lib}/lib/libclang.so";
  env.TEST_MODEL = fetchurl {
    name = "gemma-2-2b-it-Q5_K_M.gguf";
    url = "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q5_K_M.gguf";
    sha256 = "1njh254wpsg2j4wi686zabg63n42fmkgdmf9v3cl1zbydybdardy";
  };
  env.TEST_EMBEDDINGS_MODEL = fetchurl {
    name = "bge-small-en-v1.5-q8_0.gguf";
    url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
    sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
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
