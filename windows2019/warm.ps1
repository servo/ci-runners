$env:CARGO_BUILD_RUSTC = 'rustc'
cd C:\a\servo\servo

# Workaround for “Error downloading moztools-4.0: [SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get local issuer certificate (_ssl.c:1000). The failing URL was: https://github.com/servo/servo-build-deps/releases/download/msvc-deps/moztools-4.0.zip”
# Note: “.exe” avoids Invoke-WebRequest alias
curl.exe -I https://github.com

.\mach fetch
.\mach bootstrap-gstreamer

# Like `mach bootstrap`, but doesn’t require closing choco’s conhost window manually (servo#32342)
choco install -y support\windows\chocolatey.config
. C:\init\refreshenv.ps1

.\mach bootstrap --skip-platform
.\mach build --release
