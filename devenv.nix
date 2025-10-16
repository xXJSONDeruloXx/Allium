{ pkgs, lib, ... }:
let
  crossPkg = pkgs.callPackage ./cross.nix { };
in
{
  env = {
    NIX_STORE = "/nix/store";
    CROSS_CUSTOM_TOOLCHAIN = "1";
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
    LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.libclang.lib ];
  };

  enterShell = ''
    export PATH="${crossPkg}/bin:$PATH"
  '';

  packages = with pkgs; [
    rustup
    docker
    sdl2-compat # Simulator currently crashes immediately: https://github.com/libsdl-org/sdl2-compat/issues/508
    libclang
    inetutils
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
    targets = [
      "arm-unknown-linux-gnueabihf"
    ];
  };
}
