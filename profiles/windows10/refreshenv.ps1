# Configure PATH with Chocolatey, resetting any other changes made previously.
# We use the PowerShell refreshenv here, because refreshenv.cmd won’t work.
# <https://stackoverflow.com/a/46-760714>
$env:ChocolateyInstall = Convert-Path "$((Get-Command choco).Path)\..\.."
Import-Module "$env:ChocolateyInstall\helpers\chocolateyProfile.psm1"
refreshenv

# Now set PATH and LIBCLANG_PATH in the same way as a typical Windows build job.
# Doing this in the image avoids breaking incremental builds with `cargo build -v` reasons like:
# > Dirty ring v0.17.8: the env variable PATH changed
# > Dirty bindgen v0.69.4: the env variable LIBCLANG_PATH changed
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PATH = "$env:LOCALAPPDATA\Programs\Python\Python310;$env:PATH"
$env:PATH = "C:\Program Files (x86)\WiX Toolset v3.11\bin;$env:PATH"
