<#
.SYNOPSIS
    Packages Skyrim Companion into SkyrimCompanion.zip for other players.

.DESCRIPTION
    Builds the Rust apps in release mode and zips what a player needs:

      SkyrimCompanion.zip
        App\skyrim-companion.exe            the companion app (run this on the second monitor)
        App\map-extract.exe                 builds App\assets\skyrim_map.png from the player's own game files
        App\create-map.cmd                  double-click to run map-extract (finds the game by itself)
        App\assets\                         where the map image goes (included only with -IncludeMap)
        App\README.txt                      setup steps
        Data\SKSE\Plugins\SkyrimCompanionExporter.dll   the SKSE plugin; copy Data into the game folder

    companion.toml and the per-user files in %APPDATA%\Skyrim Companion are created on first run,
    so they are not packaged. The exes link the C runtime statically (.cargo\config.toml), so no
    Visual C++ Redistributable is needed for them.

    Every run adds 1 to the patch number in version.txt (0.1.0 -> 0.1.1 -> ...) and stamps it
    into both exes (file and product version, see build-support\version_resource.rs) and the SKSE
    plugin (skse-plugin\CMakeLists.txt). A failed run puts the old number back. Edit version.txt
    to change the major or minor number.

    The SKSE plugin is rebuilt with skse-plugin\build-local.cmd when that file exists (it holds
    this PC's Visual Studio + vcpkg setup and is git-ignored). Without it, build the plugin
    yourself after the version bump (`cmake --build --preset release` in skse-plugin); the
    script warns when the packaged plugin's version doesn't match.

.PARAMETER IncludeMap
    Also package assets\skyrim_map.png. It is made from Bethesda's terrain textures, so only
    share it with people who own the game.

.EXAMPLE
    .\create-release.ps1
    .\create-release.ps1 -IncludeMap
#>
param(
    [switch]$IncludeMap,
    [string]$Output = "SkyrimCompanion.zip"
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$release = Join-Path $root "target\release"
$plugin = Join-Path $root "skse-plugin\build\release\SkyrimCompanionExporter.dll"
$map = Join-Path $root "assets\skyrim_map.png"
$pluginBuild = Join-Path $root "skse-plugin\build-local.cmd"

# Fail before touching anything if the zip can't be replaced (e.g. open in 7-Zip).
$zip = Join-Path $root $Output
if (Test-Path $zip) {
    try {
        [IO.File]::Open($zip, 'Open', 'ReadWrite', 'None').Close()
    } catch {
        throw "$zip is open in another program; close it and run this again."
    }
}

# --- version: major.minor.patch, patch + 1 ---
$verFile = Join-Path $root "version.txt"
if (-not (Test-Path $verFile)) { Set-Content $verFile "0.0.0" -Encoding ASCII }
$oldVer = (Get-Content $verFile -TotalCount 1).Trim()
if ($oldVer -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
    throw "version.txt must contain major.minor.patch, found '$oldVer'"
}
$newVer = "$($Matches[1]).$($Matches[2]).$([int]$Matches[3] + 1)"
Set-Content $verFile $newVer -Encoding ASCII
$bumped = $true
Write-Host "Version $oldVer -> $newVer"
# Any failure from here on puts the old number back. (A trap covers the whole script, hence the flag.)
trap {
    if ($bumped) {
        Set-Content $verFile $oldVer -Encoding ASCII
        Write-Host "Release FAILED, version.txt left at $oldVer"
    }
    break
}

Write-Host "Building release executables..."
Push-Location $root
# cargo reports progress on stderr, which Windows PowerShell turns into errors under "Stop".
$ErrorActionPreference = "Continue"
try {
    cargo build --release -p skyrim-companion -p map-extract
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    $ErrorActionPreference = "Stop"
    Pop-Location
}

if (Test-Path $pluginBuild) {
    Write-Host "Building the SKSE plugin..."
    $ErrorActionPreference = "Continue"
    cmd /c "`"$pluginBuild`""
    $code = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    if ($code -ne 0) { throw "SKSE plugin build failed" }
}
if (-not (Test-Path $plugin)) {
    throw "SKSE plugin not found at $plugin. Build it first (see README.md)."
}
$pluginVer = (Get-Item $plugin).VersionInfo.ProductVersion
if ($pluginVer -notlike "$newVer*") {
    Write-Warning "The SKSE plugin is version '$pluginVer', not $newVer. Rebuild it (skse-plugin) and run this again."
}
if ($IncludeMap -and -not (Test-Path $map)) {
    throw "Map image not found at $map. Make it with map-extract first, or leave out -IncludeMap."
}

# Lay the files out in a staging folder, then zip it.
$stage = Join-Path $root "target\release-package"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
$app = Join-Path $stage "App"
$plugins = Join-Path $stage "Data\SKSE\Plugins"
New-Item -ItemType Directory -Force (Join-Path $app "assets"), $plugins | Out-Null

Copy-Item (Join-Path $release "skyrim-companion.exe") $app
Copy-Item (Join-Path $release "map-extract.exe") $app
Copy-Item $plugin $plugins

# Double-click helper: runs map-extract from the App folder and keeps the window open.
$createMap = @'
@echo off
rem Builds assets\skyrim_map.png from your own Skyrim install (found automatically).
rem To use another install: create-map.cmd "D:\Games\Skyrim Special Edition\Data"
"%~dp0map-extract.exe" --out "%~dp0assets\skyrim_map.png" %*
pause
'@
Set-Content -Path (Join-Path $app "create-map.cmd") -Value $createMap -Encoding ASCII
if ($IncludeMap) {
    Copy-Item $map (Join-Path $app "assets")
}

$readme = @"
Skyrim Companion $newVer
================

A second-monitor companion for Skyrim Special Edition / Anniversary Edition (1.6.x):
time, level, wanted status, gold and weight, a compass, a world map with your
trail and quest markers, and your quest journal.

Requirements
------------
- SKSE64 for your game version
- Address Library for SKSE Plugins (Nexus mod 32444)

Install
-------
1. Copy the Data folder from this zip into your Skyrim Special Edition folder
   (it adds Data\SKSE\Plugins\SkyrimCompanionExporter.dll), or install the zip
   with your mod manager.
2. Put the App folder anywhere, e.g. C:\Games\SkyrimCompanion.
3. Double-click App\create-map.cmd once to build the world map image from
   your own game files (a few seconds; see "Map image" below).
4. Start Skyrim through skse64_loader.exe, then run App\skyrim-companion.exe.
   The app connects to the game on 127.0.0.1:47311.

Settings
--------
- companion.toml is created next to the app on first run: window position
  (set x/y inside your second monitor), full screen, compass mode, map bounds.
- Edit > Settings: font and size, map history reset.
- F11 switches full screen on and off. File > Exit closes the app.
- Saved per user in %APPDATA%\Skyrim Companion: settings, each character's
  map trail, event points and markers (history\<name>.json), and run.log.

Map image
---------
The map needs App\assets\skyrim_map.png, which is not shipped because it is
made from the game's own textures. Without it the map shows a grid.

create-map.cmd builds it: it finds Skyrim Special Edition (Steam, GOG or the
Bethesda registry entry), reads the terrain LOD textures straight from the
game's archives (Skyrim - Textures7.bsa) and writes assets\skyrim_map.png.
Loose LOD files from xLODGen or DynDOLOD are used instead when present, and
an xLODGen terrain heightmap adds proper water to the sea.

If the game is installed somewhere it can't find, pass the Data folder:

  create-map.cmd "D:\Games\Skyrim Special Edition\Data"

Run map-extract.exe --help for all options.
"@
Set-Content -Path (Join-Path $app "README.txt") -Value $readme -Encoding ASCII

if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Remove-Item $stage -Recurse -Force

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "Created $zip ($size MB), version $newVer"
