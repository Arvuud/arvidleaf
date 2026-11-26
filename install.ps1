<#
.SYNOPSIS
    Installs arvidleaf on Windows 10/11 with everything it needs.

.DESCRIPTION
    1. Tools via winget: MiKTeX (LaTeX, latexmk, biber, synctex), Strawberry Perl
       (latexmk needs Perl), Poppler (PDF preview, DOI in PDFs), SQLite (Zotero
       import), Rust (rustup) and the Visual Studio C++ Build Tools (linker for Rust).
       curl (DOI lookup) comes with Windows 10/11.
    2. Release build of this checkout.
    3. Program in %LOCALAPPDATA%\Programs\arvidleaf, Start menu entry, "Open with"
       for .tex/.bib/.md, entry in "Installed apps" (uninstall there or with -Uninstall),
       and the folder in the user PATH (arvidleaf.exe --compile works in a terminal).

    Run from the source folder in PowerShell:
        powershell -ExecutionPolicy Bypass -File .\install.ps1

.PARAMETER NoDeps
    Skip installing tools with winget.
.PARAMETER Update
    Rebuild and reinstall only (same as -NoDeps); refreshes the Start menu entry.
.PARAMETER DepsOnly
    Only install the tools.
.PARAMETER DesktopShortcut
    Also put a shortcut on the desktop.
.PARAMETER Uninstall
    Remove program, shortcuts and registry entries (the tools stay).
#>
[CmdletBinding()]
param(
    [switch]$NoDeps,
    [switch]$Update,
    [switch]$DepsOnly,
    [switch]$DesktopShortcut,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$MinRust = [version]'1.95'
$Dest = Join-Path $env:LOCALAPPDATA 'Programs\arvidleaf'
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\arvidleaf.lnk'
$DesktopLnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'arvidleaf.lnk'
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\arvidleaf'
$AppKey = 'HKCU:\Software\Classes\Applications\arvidleaf.exe'

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host $msg -ForegroundColor Yellow }
function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

# Programs installed in this session are only in the registry's PATH.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin'
    $env:Path = (@($cargo, $machine, $user) | Where-Object { $_ }) -join ';'
}

function Add-UserPath($dir) {
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()
    if ($user) { $parts = $user.Split(';') | Where-Object { $_ } }
    if ($parts -notcontains $dir) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $dir) -join ';'), 'User')
    }
    Update-SessionPath
}

function Remove-UserPath($dir) {
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($user) {
        $parts = $user.Split(';') | Where-Object { $_ -and $_ -ne $dir }
        [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
    }
}

function Winget-Install($id, [string[]]$extra = @()) {
    $wargs = @('install', '--id', $id, '-e', '--silent', '--accept-source-agreements', '--accept-package-agreements') + $extra
    & winget @wargs
    # 0 = installed, -1978335189 = already installed / no newer version
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
        Warn "winget could not install $id (exit code $LASTEXITCODE)."
    }
}

function Test-Rust {
    if (-not (Have 'rustc')) { return $false }
    $v = (& rustc --version) -replace '^rustc (\d+\.\d+).*', '$1'
    return ([version]$v -ge $MinRust)
}

# Visual C++ linker present (Build Tools or Visual Studio)?
function Test-MsvcLinker {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}

function Remove-Installation {
    foreach ($l in @($StartMenu, $DesktopLnk)) { if (Test-Path $l) { Remove-Item $l -Force } }
    foreach ($k in @($UninstallKey, $AppKey)) { if (Test-Path $k) { Remove-Item $k -Recurse -Force } }
    foreach ($ext in @('.tex', '.bib', '.md')) {
        $k = "HKCU:\Software\Classes\$ext\OpenWithList\arvidleaf.exe"
        if (Test-Path $k) { Remove-Item $k -Recurse -Force }
    }
    Remove-UserPath $Dest
    if (Test-Path $Dest) { Remove-Item $Dest -Recurse -Force }
}

# A .ico that holds the PNG as is (supported since Windows Vista).
function Write-Ico($png, $ico) {
    $bytes = [IO.File]::ReadAllBytes($png)
    $ms = New-Object IO.MemoryStream
    $w = New-Object IO.BinaryWriter($ms)
    $w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]1)   # header: icon, 1 image
    $w.Write([Byte]0); $w.Write([Byte]0)                              # 256 x 256
    $w.Write([Byte]0); $w.Write([Byte]0)                              # no palette
    $w.Write([UInt16]1); $w.Write([UInt16]32)                         # planes, bits
    $w.Write([UInt32]$bytes.Length); $w.Write([UInt32]22)             # size, offset
    $w.Write($bytes)
    $w.Flush()
    [IO.File]::WriteAllBytes($ico, $ms.ToArray())
}

function New-Shortcut($lnk, $target, $icon) {
    $shell = New-Object -ComObject WScript.Shell
    $s = $shell.CreateShortcut($lnk)
    $s.TargetPath = $target
    $s.WorkingDirectory = $env:USERPROFILE
    $s.IconLocation = "$icon,0"
    $s.Description = 'Local LaTeX and Markdown editor with live PDF preview'
    $s.Save()
}

# ------------------------------------------------------------------ uninstall
if ($Uninstall) {
    if (-not (Test-Path $Dest)) { Write-Host 'arvidleaf is not installed.'; exit 1 }
    Remove-Installation
    Write-Host 'arvidleaf removed (MiKTeX, Poppler, SQLite, Perl and Rust stay installed).'
    exit 0
}

if ($Update) { $NoDeps = $true }
Update-SessionPath

# ------------------------------------------------------------------ tools
if (-not $NoDeps) {
    if (-not (Have 'winget')) {
        Warn 'winget is missing. Install "App Installer" from the Microsoft Store, then run this script again.'
        exit 1
    }
    Step 'Installing MiKTeX, Strawberry Perl, Poppler and SQLite'
    Winget-Install 'MiKTeX.MiKTeX'
    Winget-Install 'StrawberryPerl.StrawberryPerl'
    Winget-Install 'oschwartz10612.Poppler'
    Winget-Install 'SQLite.SQLite'
    Update-SessionPath

    if (Have 'initexmf') {
        # missing LaTeX packages are installed on first use instead of asking in a dialog
        & initexmf --set-config-value '[MPM]AutoInstall=1' | Out-Null
    } else {
        Warn 'MiKTeX is not in PATH yet; sign out and in again, then run: initexmf --set-config-value [MPM]AutoInstall=1'
    }

    # Poppler from winget is a portable zip: put its bin folder into PATH if needed
    if (-not (Have 'pdftoppm')) {
        $pkgs = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
        $exe = Get-ChildItem -Path $pkgs -Filter 'pdftoppm.exe' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($exe) {
            Add-UserPath $exe.DirectoryName
        } else {
            Warn 'pdftoppm.exe not found; install Poppler for Windows and add its bin folder to PATH.'
        }
    }
    # SQLite from winget is portable too (winget normally links sqlite3 into its Links folder)
    if (-not (Have 'sqlite3')) {
        $pkgs = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
        $exe = Get-ChildItem -Path $pkgs -Filter 'sqlite3.exe' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($exe) { Add-UserPath $exe.DirectoryName } else { Warn 'sqlite3.exe not found; the Zotero import needs it (optional).' }
    }

    if (-not (Test-MsvcLinker)) {
        Step 'Installing the Visual Studio C++ Build Tools (Rust needs the linker; this takes a while)'
        Winget-Install 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    }

    if (-not (Test-Rust)) {
        Step "Installing Rust $MinRust or newer"
        if (-not (Have 'rustup')) { Winget-Install 'Rustlang.Rustup'; Update-SessionPath }
        if (Have 'rustup') {
            & rustup toolchain install stable --profile minimal
            & rustup default stable
        }
        Update-SessionPath
    }
}

if ($DepsOnly) { exit 0 }

# ------------------------------------------------------------------ build
if (-not (Test-Rust)) {
    $found = 'none'
    if (Have 'rustc') { $found = & rustc --version }
    Warn "Rust $MinRust or newer is needed (found: $found). Run without -NoDeps/-Update, or: rustup update stable"
    exit 1
}
Step 'Building arvidleaf (release)'
Push-Location $Root
try {
    & cargo build --release -p arvidleaf
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally {
    Pop-Location
}
$exe = Join-Path $Root 'target\release\arvidleaf.exe'
$version = ((& $exe --version) -replace '^arvidleaf ', '').Trim()

# ------------------------------------------------------------------ install
Step "Installing arvidleaf $version to $Dest"
if (Get-Process -Name 'arvidleaf' -ErrorAction SilentlyContinue) {
    Warn 'arvidleaf is running - close it first, then run the script again.'
    exit 1
}
Remove-Installation
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item $exe (Join-Path $Dest 'arvidleaf.exe')
foreach ($f in @('README.md', 'CHANGELOG.md', 'LICENSE-MIT', 'LICENSE-APACHE')) {
    Copy-Item (Join-Path $Root $f) $Dest
}
Set-Content -Path (Join-Path $Dest 'VERSION') -Value $version -Encoding ASCII
$ico = Join-Path $Dest 'arvidleaf.ico'
Write-Ico (Join-Path $Root 'icons\arvidleaf-256.png') $ico
$target = Join-Path $Dest 'arvidleaf.exe'

# uninstaller next to the program ("Installed apps" calls it)
$un = Join-Path $Dest 'uninstall.ps1'
Copy-Item $MyInvocation.MyCommand.Path $un

New-Shortcut $StartMenu $target $ico
if ($DesktopShortcut) { New-Shortcut $DesktopLnk $target $ico }

# "Open with" for LaTeX, BibTeX and Markdown files (does not change the default app)
New-Item -Force -Path "$AppKey\shell\open\command" | Out-Null
Set-ItemProperty -Path "$AppKey\shell\open\command" -Name '(default)' -Value "`"$target`" `"%1`""
Set-ItemProperty -Path $AppKey -Name 'FriendlyAppName' -Value 'arvidleaf'
New-Item -Force -Path "$AppKey\SupportedTypes" | Out-Null
foreach ($ext in @('.tex', '.bib', '.md')) {
    Set-ItemProperty -Path "$AppKey\SupportedTypes" -Name $ext -Value ''
    New-Item -Force -Path "HKCU:\Software\Classes\$ext\OpenWithList\arvidleaf.exe" | Out-Null
}

New-Item -Force -Path $UninstallKey | Out-Null
$props = @{
    DisplayName     = 'arvidleaf'
    DisplayVersion  = $version
    Publisher       = 'arvidleaf'
    DisplayIcon     = $ico
    InstallLocation = $Dest
    UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$un`" -Uninstall"
}
foreach ($k in $props.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $props[$k] }
Set-ItemProperty -Path $UninstallKey -Name 'NoModify' -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name 'NoRepair' -Value 1 -Type DWord

Add-UserPath $Dest

# ------------------------------------------------------------------ check
Step 'Checking the setup'
$missing = $false
foreach ($t in @('latexmk', 'pdflatex', 'pdftoppm', 'pdfinfo', 'synctex', 'biber', 'perl')) {
    if (Have $t) { Write-Host "  ok       $t" } else { Write-Host "  missing  $t"; $missing = $true }
}
# optional: reference library (DOI in PDFs, online lookup, Zotero import)
# (curl.exe: in Windows PowerShell "curl" is an alias of Invoke-WebRequest)
foreach ($t in @('pdftotext', 'curl.exe', 'sqlite3')) {
    if (Have $t) { Write-Host "  ok       $t" } else { Write-Host "  optional $t (reference library)" }
}
if ($missing) { Warn 'Some tools are missing - sign out and in again (PATH), or see Help > System Check in the app.' }
Write-Host "`nStart arvidleaf from the Start menu. Uninstall: Settings > Apps > Installed apps, or .\install.ps1 -Uninstall"
