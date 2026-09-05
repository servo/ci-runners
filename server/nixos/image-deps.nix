{
  fetchurl,
  lib,
  linkFarm,
  qemu,
  runCommand,
  writeText,
  openssl,
}: let
  jammy-server-cloudimg-amd64_img = fetchurl {
    url = "https://cloud-images.ubuntu.com/jammy/20260802/jammy-server-cloudimg-amd64.img";
    hash = "sha256-v0voTtHPJV6O8g5Y2yzg0dVliCxV6MePug0lxuMu+UU=";
  };
in linkFarm "image-deps" {
  "cargo-config.toml" = writeText "cargo-config.toml" ''
    [build]
    incremental = true
  '';

  "macos13/rustup-init" = fetchurl {
    url = "https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-apple-darwin/rustup-init";
    hash = "sha256-5LH57GE4YSMiR+DLY2HJuxqGUl1ijs1Ln+rcnvngwig=";
  };
  "macos13/actions-runner-osx-x64.tar.gz" = fetchurl {
    url = "https://github.com/actions/runner/releases/download/v2.336.0/actions-runner-osx-x64-2.336.0.tar.gz";
    hash = "sha256-95xDIydhyklfwY31ULsoZaqZmEs3wXPAqh+MCdDVSP4=";
  };
  "macos13/uv-installer.sh" = fetchurl {
    url = "https://github.com/astral-sh/uv/releases/download/0.11.26/uv-installer.sh";
    hash = "sha256-kvqQhdJMIUu0RFzB2owVypzKjP+zRyYkD6CMUwLpTMw=";
  };
  "macos13/install-xcode-clt.sh" = fetchurl {
    url = "https://raw.githubusercontent.com/actions/runner-images/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh";
    hash = "sha256-LJDSx28tN171QE1nUQiU67+ReUCu9QF+vuTGuMEMQvs=";
  };
  "macos13/install-homebrew.sh" = fetchurl {
    url = "https://raw.githubusercontent.com/Homebrew/install/9a01f1f361cc66159c31624df04b6772d26b7f98/install.sh";
    hash = "sha256-owufvw1cLP8+sdBkPM7uMNi6bqG7e8q/YNMYi9Yua6Y=";
  };

  "ubuntu2204/jammy-server-cloudimg-amd64.raw" = runCommand "jammy-server-cloudimg-amd64.raw" {} ''
    ${qemu}/bin/qemu-img convert -f qcow2 -O raw ${jammy-server-cloudimg-amd64_img} $out
  '';
  "ubuntu2204/rustup-init" = fetchurl {
    url = "https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-unknown-linux-gnu/rustup-init";
    hash = "sha256-ozOfsATD0LuYYroLzgAYYf5cvenBDRZZHrPznubNPn8=";
  };
  "ubuntu2204/actions-runner-linux-x64.tar.gz" = fetchurl {
    url = "https://github.com/actions/runner/releases/download/v2.336.0/actions-runner-linux-x64-2.336.0.tar.gz";
    hash = "sha256-BM8L4a/0w+w1VEZsORJMolDj7/2Ic7t+jWhTWqlQXV0=";
  };
  "ubuntu2204/uv-installer.sh" = fetchurl {
    url = "https://github.com/astral-sh/uv/releases/download/0.11.26/uv-installer.sh";
    hash = "sha256-kvqQhdJMIUu0RFzB2owVypzKjP+zRyYkD6CMUwLpTMw=";
  };

  "windows10/cacert.pfx" = runCommand "cacert.pfx" {} ''
    ${openssl}/bin/openssl pkcs12 -export -nokeys -passout pass:servo -out $out -in ${fetchurl {
      url = "https://curl.se/ca/cacert.pem";
      hash = "sha256-9m3/G9+PlgYLgXeXb4t9klS8ibxNuTPXafc4TShIC8k=";
    }}
  '';
  "windows10/virtio-win-0.1.240.iso" = fetchurl {
    url = "https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.240-1/virtio-win-0.1.240.iso";
    hash = "sha256-69SCWGaPf3jgJu0nbCip0Z2D4CD/oICtaZENyGu8vMY=";
  };
  "windows10/Win10_22H2_English_x64v1.iso" = fetchurl {
    # Sometimes it can be faster to lock in a specific mirror after trying them with `curl -I` and `curl`.
    # You can also check how fast the mirror selected by Nix is going with `sudo pv -d $(pidof curl)`.
    # url = "https://ia601808.us.archive.org/3/items/Win10_22H2_English_x64v1/Win10_22H2_English_x64v1.iso";
    url = "https://archive.org/download/Win10_22H2_English_x64v1/Win10_22H2_English_x64v1.iso";
    hash = "sha256-pvRwym0zHrNTuBXAQ+Mno0f1lPN/9SXxd2Rzj+gShS4=";
  };
  "windows10/python-3.10.11-amd64.exe" = fetchurl {
    url = "https://www.python.org/ftp/python/3.10.11/python-3.10.11-amd64.exe";
    hash = "sha256-2N7eUAVWS0CLpQMXEIt2XtnDxRA0KlmPn9QmgcvgZIs=";
  };
  "windows10/uv-installer.ps1" = fetchurl {
    url = "https://github.com/astral-sh/uv/releases/download/0.11.26/uv-installer.ps1";
    hash = "sha256-N7O8lHCKrLMVJU38ad8ybkC2cNP0eFy94YmQM2nb0hs=";
  };
  "windows10/ndp48-x86-x64-allos-enu.exe" = fetchurl {
    url = "https://download.visualstudio.microsoft.com/download/pr/2d6bb6b2-226a-4baa-bdec-798822606ff1/8494001c276a4b96804cde7829c04d7f/ndp48-x86-x64-allos-enu.exe";
    hash = "sha256-aMmYao3MAhTZCaofMb7p+1Rhu4Oe3KmWp1sI3f/BSD8=";
  };
  "windows10/vswhere.exe" = fetchurl {
    url = "https://github.com/microsoft/vswhere/releases/download/3.1.7/vswhere.exe";
    hash = "sha256-xU87fJFk6poNuGQegezdqAwmZO9aR8QZFAb4SMwHxmI=";
  };
  "windows10/vs_community.exe" = fetchurl {
    url = "https://aka.ms/vs/17/release/vs_community.exe";
    hash = "sha256-mjMiorG4xjL8X8EFxSmQxxFY+cqP63NpDcLhMXp/Qbk=";
  };
  "windows10/rustup-init.exe" = fetchurl {
    url = "https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-pc-windows-msvc/rustup-init.exe";
    hash = "sha256-e4MDmhuTBbDFDyOy4vAzGbjXhZsoEG5JuoLAbYEonfY=";
  };
  "windows10/actions-runner-win-x64.zip" = fetchurl {
    url = "https://github.com/actions/runner/releases/download/v2.336.0/actions-runner-win-x64-2.336.0.zip";
    hash = "sha256-1ZEjpDAD41ewgFtdD2EdC9L2WrZ9Ub0HDdTnoPaFwWI=";
  };
  "windows10/Git-2.45.1-64-bit.exe" = fetchurl {
    url = "https://github.com/git-for-windows/git/releases/download/v2.45.1.windows.1/Git-2.45.1-64-bit.exe";
    hash = "sha256-GytY+1Fklf63A1OqkdojC+CitKoBrMO8BH7h/khGvE4=";
  };
  "windows10/software.reg" = writeText "software.reg" (lib.readFile ../../static/windows10/software.reg);
  "windows10/system.reg" = writeText "system.reg" (lib.readFile ../../static/windows10/system.reg);
}
