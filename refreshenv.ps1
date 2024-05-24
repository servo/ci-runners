# Get the PowerShell refreshenv, because refreshenv.cmd won’t work
# <https://stackoverflow.com/a/46760714>
$env:ChocolateyInstall = Convert-Path "$((Get-Command choco).Path)\..\.."
Import-Module "$env:ChocolateyInstall\helpers\chocolateyProfile.psm1"
refreshenv
