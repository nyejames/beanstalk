param(
    [string]$Version = $env:BST_VERSION,
    [string]$BinDir = $env:BIN_DIR,
    [switch]$AddToPath
)

$ErrorActionPreference = "Stop"

$Repo = "nyejames/beanstalk"
$BinName = "bean"
$BinaryName = "bean.exe"

function Fail($Message) {
    Write-Error "error: $Message"
    exit 1
}

function Resolve-Version {
    param([string]$RequestedVersion)

    if ($RequestedVersion -and $RequestedVersion -ne "latest") {
        return $RequestedVersion
    }

    # GitHub's latest endpoint ignores prereleases, so query the releases list.
    $ReleasesUrl = "https://api.github.com/repos/$Repo/releases?per_page=1"
    $Releases = Invoke-RestMethod -Uri $ReleasesUrl

    if (-not $Releases -or -not $Releases[0].tag_name) {
        Fail "could not resolve latest release version"
    }

    return $Releases[0].tag_name
}

function Resolve-Target {
    $Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

    switch ($Architecture) {
        "X64" {
            return "x86_64-pc-windows-msvc"
        }

        default {
            Fail "unsupported Windows architecture: $Architecture. Current release artifacts only support x86_64 Windows."
        }
    }
}

function Get-ExpectedChecksum {
    param(
        [string]$ChecksumsPath,
        [string]$ArchiveName
    )

    foreach ($Line in Get-Content $ChecksumsPath) {
        $Parts = $Line -split "\s+"

        if ($Parts.Count -ge 2 -and $Parts[1] -eq $ArchiveName) {
            return $Parts[0]
        }
    }

    Fail "checksum not found for $ArchiveName"
}

function Add-BinDirToUserPath {
    param([string]$Directory)

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if (-not $UserPath) {
        $UserPath = ""
    }

    $PathParts = $UserPath -split ";" | Where-Object { $_ -ne "" }

    if ($PathParts -contains $Directory) {
        return
    }

    $NewPath = if ($UserPath.EndsWith(";") -or $UserPath.Length -eq 0) {
        "$UserPath$Directory"
    } else {
        "$UserPath;$Directory"
    }

    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    $env:Path = "$env:Path;$Directory"

    Write-Host "Added $Directory to your user PATH."
    Write-Host "Restart your terminal for PATH changes to apply everywhere."
}

if (-not $Version) {
    $Version = "latest"
}

if (-not $BinDir) {
    $BinDir = Join-Path $env:LOCALAPPDATA "Programs\Beanstalk\bin"
}

$Version = Resolve-Version $Version
$Target = Resolve-Target

$ArchiveName = "$BinName-$Version-$Target.zip"
$ReleaseBaseUrl = "https://github.com/$Repo/releases/download/$Version"
$ArchiveUrl = "$ReleaseBaseUrl/$ArchiveName"
$ChecksumsUrl = "$ReleaseBaseUrl/SHA256SUMS"

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) "beanstalk-install-$([System.Guid]::NewGuid())"
$ArchivePath = Join-Path $TempDir $ArchiveName
$ChecksumsPath = Join-Path $TempDir "SHA256SUMS"
$ExtractDir = Join-Path $TempDir "extract"

try {
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

    Write-Host "Installing Beanstalk CLI"
    Write-Host "Version: $Version"
    Write-Host "Target:  $Target"
    Write-Host "Binary:  $BinaryName"
    Write-Host "Install: $(Join-Path $BinDir $BinaryName)"
    Write-Host ""

    Write-Host "Downloading $ArchiveUrl"
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ArchivePath

    Write-Host "Downloading $ChecksumsUrl"
    Invoke-WebRequest -Uri $ChecksumsUrl -OutFile $ChecksumsPath

    $ExpectedChecksum = Get-ExpectedChecksum -ChecksumsPath $ChecksumsPath -ArchiveName $ArchiveName
    $ActualChecksum = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()

    if ($ActualChecksum -ne $ExpectedChecksum.ToLowerInvariant()) {
        Fail "checksum mismatch for $ArchiveName"
    }

    Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force

    $FoundBinary = Get-ChildItem -Path $ExtractDir -Recurse -Filter $BinaryName | Select-Object -First 1

    if (-not $FoundBinary) {
        Fail "could not find $BinaryName in archive"
    }

    $InstalledBinary = Join-Path $BinDir $BinaryName
    Copy-Item -Path $FoundBinary.FullName -Destination $InstalledBinary -Force

    if ($AddToPath) {
        Add-BinDirToUserPath $BinDir
    }

    Write-Host ""
    Write-Host "Installed:"
    & $InstalledBinary --version

    $CurrentPathParts = $env:Path -split ";"

    if ($CurrentPathParts -notcontains $BinDir) {
        Write-Host ""
        Write-Host "Installed to $BinDir, but that directory is not in PATH."
        Write-Host "Run again with -AddToPath, or add this directory manually."
    }
}
finally {
    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force $TempDir
    }
}