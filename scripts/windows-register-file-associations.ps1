param(
  [string]$ExePath = (Join-Path $PSScriptRoot "Auralux.exe")
)

$ErrorActionPreference = "Stop"

$Extensions = @(
  "aac", "aif", "aiff", "alac", "ape", "caf", "dff", "dsf", "flac", "m4a",
  "mka", "mp2", "mp3", "mp4", "mpc", "oga", "ogg", "opus", "tak", "tta",
  "wav", "weba", "wma", "wv"
)
$ProgId = "Auralux.Audio"
$ClassesRoot = "HKCU:\Software\Classes"

if (!(Test-Path $ExePath)) {
  throw "Auralux.exe was not found at $ExePath"
}

$ResolvedExe = (Resolve-Path $ExePath).Path

function Set-DefaultRegistryValue {
  param(
    [string]$Path,
    [string]$Value
  )

  New-Item -Path $Path -Force | Out-Null
  (Get-Item $Path).SetValue("", $Value, [Microsoft.Win32.RegistryValueKind]::String)
}

Set-DefaultRegistryValue -Path "$ClassesRoot\$ProgId" -Value "Auralux Audio"
Set-DefaultRegistryValue -Path "$ClassesRoot\$ProgId\DefaultIcon" -Value "`"$ResolvedExe`",0"
Set-DefaultRegistryValue -Path "$ClassesRoot\$ProgId\shell\open\command" -Value "`"$ResolvedExe`" `"%1`""

foreach ($Extension in $Extensions) {
  $ExtensionKey = "$ClassesRoot\.$Extension"
  Set-DefaultRegistryValue -Path $ExtensionKey -Value $ProgId

  $OpenWithKey = "$ExtensionKey\OpenWithProgids"
  New-Item -Path $OpenWithKey -Force | Out-Null
  (Get-Item $OpenWithKey).SetValue($ProgId, "", [Microsoft.Win32.RegistryValueKind]::None)
}

if (Get-Command ie4uinit.exe -ErrorAction SilentlyContinue) {
  ie4uinit.exe -show | Out-Null
}

Write-Host "Auralux file associations registered for $($Extensions.Count) audio extensions."
