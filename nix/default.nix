{ rustPlatform
, lib
, pkg-config
, openssl
, alsa-lib
, speechd
, libclang
, callPackage
, onnxruntime
}:
let
  manifest = (lib.importTOML ../raspberry/Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  version = manifest.version;
  cargoLock.lockFile = ../Cargo.lock;
  src = lib.cleanSource ./..;

  nativeBuildInputs = [
    pkg-config
  ];
  buildInputs = [
    openssl
    alsa-lib
    speechd
    libclang.lib
    rustPlatform.bindgenHook
    onnxruntime
    (callPackage (import ./vosk-api-bin.nix) { })
  ];

  doCheck = false;

  meta = {
    description = "A voice assistant project intended to run on a Raspberry Pi";
    mainProgram = "raspberry";
    platforms = lib.platforms.unix;
  };
}
