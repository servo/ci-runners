{
  pkgs ? import <nixpkgs> {},
  image-deps,
}: with pkgs; mkShell {
  IMAGE_DEPS_DIR = image-deps;
}
