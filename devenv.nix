{ pkgs, ... }:
let
  crossPkg = pkgs.callPackage ./cross.nix { };
in
{
  env = {
    NIX_STORE = "/nix/store";
    CROSS_CUSTOM_TOOLCHAIN = "1";
  };

  enterShell = ''
    export PATH="${crossPkg}/bin:$PATH"
  '';

  packages = with pkgs; [
    rustup
    docker
    sdl2-compat # Simulator currently crashes immediately: https://github.com/libsdl-org/sdl2-compat/issues/508
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
    targets = [
      "arm-unknown-linux-gnueabihf"
    ];
  };
}
