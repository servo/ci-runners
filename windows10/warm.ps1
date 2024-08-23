cd C:\a\servo\servo

# Workaround for “Error downloading moztools-4.0: [SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get local issuer certificate (_ssl.c:1000). The failing URL was: https://github.com/servo/servo-build-deps/releases/download/msvc-deps/moztools-4.0.zip”
# Note: “.exe” avoids Invoke-WebRequest alias
curl.exe -I https://github.com

# ntfs-3g seems to write symlinks like shell.nix → etc/shell.nix differently to
# git, so reset the working tree to fix up any discrepancies.
git reset --hard

.\mach fetch
.\mach bootstrap-gstreamer

# Like `mach bootstrap`, but doesn’t require closing choco’s conhost window manually (servo#32342)
choco install -y support\windows\chocolatey.config
. C:\init\refreshenv.ps1

.\mach bootstrap --skip-platform

# Build the same way as a typical Windows build job, to allow for incremental builds.
$env:RUSTUP_WINDOWS_PATH_ADD_BIN = 1
.\mach build --use-crown --locked --release
