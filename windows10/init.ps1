# Out-Default makes the script wait for programs in the Windows subsystem to exit
# https://stackoverflow.com/a/7272390

# We need to run this on boot as Administrator, but the current user will only be Administrator on Windows Server,
# and HKLM Run can’t run elevated commands. Instead register an elevated scheduled task. This is tricky to do with
# a registry patch or `Register-ScheduledTask -xml`, because we would need to know the user’s SID and hostname.
# Note: `schtasks /Create /F` is idempotent, keyed on task name (/TN)
schtasks /Create /F /TN "servo ci init" /SC ONLOGON /RL HIGHEST /TR "powershell -NoExit C:\init\init.ps1"

# Disable Windows Defender, for better disk performance
Set-MpPreference -DisableRealtimeMonitoring $True

# Install VirtIO NIC driver (NetKVM)
# Note: installer fails without negative side effects when rerun, so the check is just there to save time
# https://stackoverflow.com/q/22496847
if (!(Test-Path C:\Windows\System32\drivers\netkvm.sys)) {
    pnputil -i -a E:\NetKVM\2k19\amd64\netkvm.inf
}

# Install Git
# Note: installer automatically uninstalls first if needed, so the check is just there to save time
if (!(Test-Path 'C:\Program Files\Git\cmd\git.exe')) {
    C:\init\Git-2.45.1-64-bit.exe /silent | Out-Default
}

# Install Python, for checkouts without servo#34504
# Note: installer is idempotent, so the check is just there to save time
if (!(Test-Path $env:LOCALAPPDATA\Programs\Python\Python310\python.exe)) {
    C:\init\python-3.10.11-amd64.exe /passive | Out-Default
}

# Install uv. Note that actually running uv relies on $env:USERPROFILE\.local\bin being on PATH,
# and that directory gets added to PATH by Chocolatey and refreshenv (see refreshenv.ps1).
if (!(Test-Path C:\Users\Administrator\.local\bin\uv.exe)) {
    . C:\init\install-uv.ps1
}

# Install rustup and the latest Rust
if (!(Test-Path C:\Users\Administrator\.rustup)) {
    C:\init\rustup-init.exe -y --quiet
}

# Install .NET 4.8 for Chocolatey
# Note: installer is idempotent, so the check is just there to save time
# Note: explicit install avoids failure in Chocolatey installer due to the required reboot
# See also: <https://learn.microsoft.com/en-us/dotnet/framework/migration-guide/how-to-determine-which-versions-are-installed#query-the-registry-using-powershell>
# See also: <https://learn.microsoft.com/en-us/dotnet/core/install/windows?tabs=net80>
# See also: the installer’s /?
if (!((Get-ItemPropertyValue -LiteralPath 'HKLM:SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full' -Name Release) -ge 528040)) {
    # /passive works on Windows Server with desktop, but not on core
    # <https://serverfault.com/a/914454>
    C:\init\ndp48-x86-x64-allos-enu.exe /norestart /q | Out-Default
    # Explicit reboot and exit to avoid running Chocolatey installer
    shutdown /r /t 0
    exit
}

# Install Chocolatey
if (!(Test-Path C:\ProgramData\chocolatey\bin\choco.exe)) {
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
}

# Install Visual Studio 2022 (17.0), with the components from the Servo book
# <https://book.servo.org/hacking/setting-up-your-environment.html#tools-for-windows>
# See also: <https://github.com/rust-lang/rustup/blob/2a5a69e0914ff419554d684ca71eb1d72c72bcb3/src/cli/self_update/windows.rs#L174>
# See also: <https://learn.microsoft.com/en-us/visualstudio/install/use-command-line-parameters-to-install-visual-studio?view=vs-2022>
# See also: the installer’s --help
if ($(C:\init\vswhere.exe -format value -property isComplete) -ne '1') {
    C:\init\vs_community.exe --wait --focusedUi --addProductLang en-us `
        --add Microsoft.VisualStudio.Component.Windows10SDK.19041 `
        --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        --add Microsoft.VisualStudio.Component.VC.ATL `
        --add Microsoft.VisualStudio.Component.VC.ATLMFC `
        --passive | Out-Default
    # FIXME: rebooting here as a speculative fix for choco hanging during warm.ps1
    shutdown /r /t 0
    exit
}

if (!(Test-Path C:\init\built_servo_once_successfully)) {
    . C:\init\warm.ps1
    if (Test-Path C:\a\servo\servo\target\release\servo.exe) {
        New-Item C:\init\built_servo_once_successfully
        shutdown /s /t 0
    }
    exit
}

# Use the actual curl shipped in Windows 1804+, not the alias for Invoke-WebRequest
$jitconfig = curl.exe -fsS http://192.168.100.1:8000/github-jitconfig | ConvertFrom-Json
if ($jitconfig -ne $null) {
    . C:\init\refreshenv.ps1
    C:\actions-runner\run.cmd --jitconfig $jitconfig
}
