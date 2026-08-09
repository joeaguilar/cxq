<#
.SYNOPSIS
    Build and install cxq from this checkout on Windows.

.DESCRIPTION
    Runs cargo install and ensures Cargo's bin directory is on the persistent
    user PATH. Rust must already be installed; rustup is available from
    https://rustup.rs or through WinGet as Rustlang.Rustup.
#>

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-PathEntries {
    param([string]$Scope)

    $value = [Environment]::GetEnvironmentVariable('Path', $Scope)
    if (-not $value) { return @() }
    return @($value -split ';' | Where-Object { $_ -ne '' })
}

function Test-OnPersistentPath {
    param([string]$Dir)

    $target = $Dir.TrimEnd('\')
    foreach ($entry in (@(Get-PathEntries 'User') + @(Get-PathEntries 'Machine'))) {
        if ($entry.TrimEnd('\') -ieq $target) { return $true }
    }
    return $false
}

function Add-ToUserPath {
    param([string]$Dir)

    if (Test-OnPersistentPath $Dir) { return $false }
    $entries = @($Dir) + @(Get-PathEntries 'User')
    [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')
    return $true
}

$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
if (-not $cargo) {
    throw 'cargo.exe was not found. Install Rust with "winget install Rustlang.Rustup", restart PowerShell, and run this installer again.'
}

$repoDir = Split-Path -Parent $MyInvocation.MyCommand.Path
& $cargo.Source install --path $repoDir --force
if ($LASTEXITCODE -ne 0) {
    throw "cargo install failed with exit code $LASTEXITCODE"
}

$cargoHome = if ($env:CARGO_HOME) {
    $env:CARGO_HOME
} elseif ($env:USERPROFILE) {
    Join-Path $env:USERPROFILE '.cargo'
} else {
    throw 'Could not resolve Cargo home (CARGO_HOME/USERPROFILE unset).'
}
$binDir = Join-Path $cargoHome 'bin'

if (Add-ToUserPath $binDir) {
    Write-Host "Added $binDir to your user PATH."
}
if (($env:Path -split ';') -notcontains $binDir) {
    $env:Path = "$binDir;$env:Path"
}

$installed = Join-Path $binDir 'cxq.exe'
if (-not (Test-Path -LiteralPath $installed)) {
    throw "cargo reported success, but $installed was not found"
}

Write-Host "Installed cxq to $installed"
& $installed --version
