# Based on the uv2nix docs:
# <https://pyproject-nix.github.io/uv2nix/install.html#classic-nix>
# <https://pyproject-nix.github.io/uv2nix/usage/getting-started.html>
# <https://pyproject-nix.github.io/uv2nix/patterns/applications.html>
{
  src,
  packageName,

  # Provided by callPackage:
  lib,
  callPackage,
  fetchFromGitHub,
  python3,
}: let
  pyproject-nix = import (fetchFromGitHub {
    owner = "pyproject-nix";
    repo = "pyproject.nix";
    rev = "02e9418fd4af638447dca4b17b1280da95527fc9";
    hash = "sha256-amLaLNwKSZPShQHzfgmc/9o76dU8xzN0743dWgvYlr8=";
  }) {
    inherit lib;
  };
  uv2nix = import (fetchFromGitHub {
    owner = "pyproject-nix";
    repo = "uv2nix";
    rev = "87de87c2486da49d22daef61fc98c490789a8e42";
    hash = "sha256-L9/jSc4YHNfeTa/iKolWtzF4A00o21YnrZmoTHAoo2U=";
  }) {
    inherit pyproject-nix lib;
  };
  pyproject-build-systems = import (fetchFromGitHub {
    owner = "pyproject-nix";
    repo = "build-system-pkgs";
    rev = "5b8e37fe0077db5c1df3a5ee90a651345f085d38";
    hash = "sha256-6nzSZl28IwH2Vx8YSmd3t6TREHpDbKlDPK+dq1LKIZQ=";
  }) {
    inherit pyproject-nix uv2nix lib;
  };
  pythonBase = callPackage pyproject-nix.build.packages {
    python = python3;
  };
  workspace = uv2nix.lib.workspace.loadWorkspace {
    workspaceRoot = src;
  };
  overlay = workspace.mkPyprojectOverlay {
    sourcePreference = "wheel";
  };
  pythonSet = pythonBase.overrideScope (
    lib.composeManyExtensions [
      # NOTE: just `.wheel`, not `.overlays.wheel`, since we aren’t using the flake
      # <https://github.com/pyproject-nix/build-system-pkgs/blob/5b8e37fe0077db5c1df3a5ee90a651345f085d38/flake.nix#L34>
      # <https://github.com/pyproject-nix/build-system-pkgs/blob/5b8e37fe0077db5c1df3a5ee90a651345f085d38/default.nix#L11>
      pyproject-build-systems.wheel
      overlay
    ]
  );
  venv = pythonSet.mkVirtualEnv "venv" workspace.deps.default;
  inherit (callPackage pyproject-nix.build.util {}) mkApplication;
in mkApplication {
  inherit venv;
  package = pythonSet."${packageName}";
}
