# Cheip OS - Lumia boot setup
# Installs BootShim and lk1st on the EFIESP partition and adds a "Cheip OS"
# entry to the Windows Phone boot menu. Windows Phone stays installed.
#
# Requirements:
#  - bootloader unlocked with WPInternals
#  - phone in Mass Storage mode (WPInternals > Manual mode > Mass Storage)
#  - BootShim.efi and emmc_appsboot.mbn in the same folder as this script
#
# Run from an elevated PowerShell:  .\cheip-lumia-setup.ps1 [-CheipDefault] [-Uninstall]

param(
    [switch]$CheipDefault,
    [switch]$Uninstall,
    [int]$Timeout = 5
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

function Fail($msg) { Write-Host "`n$msg" -ForegroundColor Red; Read-Host "Press Enter to close"; exit 1 }

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "Run this script as administrator (right click > Run as administrator)."
}

Write-Host "Cheip OS - Lumia boot setup" -ForegroundColor Cyan

# EFIESP: the volume (or MainOS subfolder) that holds efi\Microsoft\boot\BCD
$esp = $null
foreach ($d in (Get-PSDrive -PSProvider FileSystem)) {
    foreach ($p in @("$($d.Root)", "$($d.Root)EFIESP\")) {
        if (Test-Path (Join-Path $p "efi\Microsoft\boot\BCD")) { $esp = $p; break }
    }
    if ($esp) { break }
}
if (-not $esp) { Fail "Phone not found. Put it in Mass Storage mode with WPInternals and try again." }
Write-Host "EFIESP found at $esp"

$bcd = Join-Path $esp "efi\Microsoft\boot\BCD"
$bootDir = Join-Path $esp "Windows\System32\Boot"
$shimDst = Join-Path $bootDir "cheipshim.efi"
$lkDst = Join-Path $esp "emmc_appsboot.mbn"
$backup = Join-Path ([Environment]::GetFolderPath("MyDocuments")) ("CheipOS-backup-" + (Get-Date -Format "yyyyMMdd-HHmmss"))

function Find-Entry($description) {
    $out = bcdedit /store "$bcd" /enum all
    $id = $null
    foreach ($line in $out) {
        if ($line -match '^identifier\s+(\{[0-9a-fA-F-]+\})') { $id = $Matches[1] }
        if ($line -match "^description\s+$([regex]::Escape($description))\s*$") { return $id }
    }
    return $null
}

New-Item -ItemType Directory -Force -Path $backup | Out-Null
Copy-Item $bcd (Join-Path $backup "BCD")
Write-Host "Backup of the boot configuration saved in $backup"

if ($Uninstall) {
    $entry = Find-Entry "Cheip OS"
    if ($entry) { bcdedit /store "$bcd" /delete $entry /f | Out-Null }
    Remove-Item -Force -ErrorAction SilentlyContinue $shimDst, $lkDst
    bcdedit /store "$bcd" /set "{bootmgr}" displaybootmenu no | Out-Null
    Write-Host "`nCheip OS boot entry removed. The phone boots Windows Phone only." -ForegroundColor Green
    Read-Host "Press Enter to close"
    exit 0
}

$shimSrc = Join-Path $here "BootShim.efi"
$lkSrc = Join-Path $here "emmc_appsboot.mbn"
if (-not (Test-Path $shimSrc) -or -not (Test-Path $lkSrc)) { Fail "BootShim.efi and emmc_appsboot.mbn must be next to this script." }

# The reset app entry is the template: same type of boot application
$reset = Find-Entry "Reset Phone"
if (-not $reset) { $reset = Find-Entry "Reset Phone Application" }
if (-not $reset) {
    # Identificatore noto dell'app di ripristino sui Lumia
    $known = "{bd8951c4-eabd-4c6f-aafb-4ddb4eb0469b}"
    if ((bcdedit /store "$bcd" /enum $known 2>$null) -match "identifier") { $reset = $known }
}
if (-not $reset) { Fail "Could not find the Reset Phone entry in the BCD. Nothing was changed." }

Copy-Item -Force $shimSrc $shimDst
Copy-Item -Force $lkSrc $lkDst
Write-Host "BootShim and lk1st copied"

$entry = Find-Entry "Cheip OS"
if (-not $entry) {
    $res = bcdedit /store "$bcd" /copy $reset /d "Cheip OS"
    if ("$res" -match '(\{[0-9a-fA-F-]+\})') { $entry = $Matches[1] } else { Fail "bcdedit could not create the entry: $res" }
}
bcdedit /store "$bcd" /set $entry path "\Windows\System32\Boot\cheipshim.efi" | Out-Null
bcdedit /store "$bcd" /set $entry nointegritychecks yes | Out-Null
bcdedit /store "$bcd" /set "{bootmgr}" nointegritychecks yes | Out-Null
bcdedit /store "$bcd" /set "{bootmgr}" displaybootmenu yes | Out-Null
bcdedit /store "$bcd" /set "{bootmgr}" timeout $Timeout | Out-Null
bcdedit /store "$bcd" /displayorder $entry /addfirst | Out-Null
if ($CheipDefault) { bcdedit /store "$bcd" /default $entry | Out-Null }

Write-Host "`nDone. Boot menu: Cheip OS and Windows Phone, $Timeout seconds." -ForegroundColor Green
Write-Host "Now: unplug the phone, then hold Power until it vibrates to restart."
Write-Host "Use the volume keys to choose Cheip OS and Power to confirm."
Read-Host "Press Enter to close"
