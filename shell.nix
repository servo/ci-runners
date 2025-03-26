{
  pkgs ? import <nixpkgs> {},
  image-deps,
  monitor,
}: with pkgs; mkShell {
  IMAGE_DEPS_DIR = image-deps;

  packages = [
    monitor
  ];
}
