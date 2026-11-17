<#
.SYNOPSIS
Install the rqx binary for this machine from its GitHub release.

.DESCRIPTION
Works out the release's target triple from the processor architecture, downloads the
tarball and SHA256SUMS over HTTPS into a throwaway directory, checks the tarball
against SHA256SUMS, and only then copies rqx.exe into the install directory. A
mismatch or a missing checksum stops it with nothing installed.

It never needs administrator rights and never touches PATH on its own. The user PATH
is changed only when -AddToPath is passed; otherwise the command to do it is printed.

Piped through `iex` it cannot take parameters, so RQX_VERSION and RQX_INSTALL_DIR
are read from the environment as well.

.PARAMETER Version
Release to install, such as 0.2.0. Default: the latest release. A prerelease is
installed only when named here.

.PARAMETER InstallDir
Directory for rqx.exe. Default: $env:LOCALAPPDATA\rqx\bin

.PARAMETER DryRun
Print every step and the URLs involved, and change nothing.

.PARAMETER AddToPath
Append InstallDir to the user PATH.

.EXAMPLE
irm https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.ps1 | iex

.EXAMPLE
.\install.ps1 -Version 0.2.0 -InstallDir C:\tools\rqx -AddToPath
#>
param(
    [string]$Version,
    [string]$InstallDir,
    [switch]$DryRun,
    [switch]$AddToPath
)

$script:Repository = 'jadhamwi21/rqx'
$script:ReleasesUrl = "https://github.com/$script:Repository/releases"
$script:CargoFallback = "cargo install --git https://github.com/$script:Repository --locked rqx"

function Write-Step {
    param([string]$Message)
    Write-Output "  $Message"
}

function Exit-WithFallback {
    param([string]$Message)
    throw "$Message`nTo build from source instead (needs the Rust toolchain):`n  $script:CargoFallback"
}

function Get-RqxTriple {
    # A 32-bit PowerShell on a 64-bit Windows reports x86 here and names the real
    # architecture in the second variable.
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }

    switch ($architecture) {
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        'ARM64' { return 'aarch64-pc-windows-msvc' }
        default { Exit-WithFallback "no rqx binary is published for the $architecture architecture" }
    }
}

# The final URL of /releases/latest ends in /tag/vX.Y.Z. Reading it from the redirect
# keeps this off the GitHub API and its rate limit.
function Get-LatestRqxVersion {
    try {
        $response = Invoke-WebRequest -Uri "$script:ReleasesUrl/latest" -UseBasicParsing
    } catch {
        Exit-WithFallback "could not reach $script:ReleasesUrl/latest: $($_.Exception.Message)"
    }

    # Windows PowerShell and PowerShell 7 keep the final URL in different places.
    $baseResponse = $response.BaseResponse
    if ($baseResponse.PSObject.Properties['ResponseUri']) {
        $finalUrl = $baseResponse.ResponseUri.AbsoluteUri
    } else {
        $finalUrl = $baseResponse.RequestMessage.RequestUri.AbsoluteUri
    }

    if ($finalUrl -notmatch '/releases/tag/v?(?<version>[^/]+)$') {
        Exit-WithFallback "$script:Repository has no published release yet"
    }
    $latest = $Matches['version']
    if ($latest -match '-') {
        throw "the latest release is the prerelease $latest; name it with -Version to install it"
    }
    return $latest
}

function Get-ExpectedChecksum {
    param([string]$ChecksumsPath, [string]$ArchiveName)

    foreach ($line in Get-Content -Path $ChecksumsPath) {
        if ($line -match '^(?<hash>[0-9a-fA-F]{64}) [ *](?<name>.+)$' -and $Matches['name'] -eq $ArchiveName) {
            return $Matches['hash']
        }
    }
    throw "SHA256SUMS has no valid checksum for $ArchiveName; nothing was installed"
}

function Assert-Checksum {
    param([string]$ArchivePath, [string]$ChecksumsPath, [string]$ArchiveName)

    $expected = Get-ExpectedChecksum -ChecksumsPath $ChecksumsPath -ArchiveName $ArchiveName
    $actual = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash
    if ($actual -ne $expected) {
        throw "$ArchiveName hashes to $actual but SHA256SUMS says $expected; nothing was installed"
    }
}

function Get-TarPath {
    # The tar that ships with Windows, by full path: a GNU tar earlier on PATH (Git
    # for Windows has one) reads `C:\...` as a remote host.
    $systemTar = Join-Path $env:SystemRoot 'System32\tar.exe'
    if (Test-Path -Path $systemTar) { return $systemTar }
    $found = Get-Command -Name tar -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    throw 'tar is not available; it ships with Windows 10 version 1803 and later'
}

function Test-OnUserPath {
    param([string]$Directory)

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { return $false }
    $wanted = $Directory.TrimEnd('\')
    foreach ($entry in $userPath.Split(';')) {
        if ($entry.TrimEnd('\') -eq $wanted) { return $true }
    }
    return $false
}

function Write-PathAdvice {
    param([string]$Directory, [bool]$Append)

    if (Test-OnUserPath -Directory $Directory) { return }

    if ($Append) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $combined = if ($userPath) { "$($userPath.TrimEnd(';'));$Directory" } else { $Directory }
        [Environment]::SetEnvironmentVariable('Path', $combined, 'User')
        Write-Output "Added $Directory to your user PATH. Open a new terminal to pick it up."
        return
    }

    Write-Output ''
    Write-Output "$Directory is not on your PATH. To add it for your user, run:"
    Write-Output "  [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$Directory', 'User')"
    Write-Output 'or install again with -AddToPath.'
}

function Install-Rqx {
    [CmdletBinding()]
    param(
        [string]$Version,
        [string]$InstallDir,
        [bool]$DryRun,
        [bool]$AddToPath
    )

    Set-StrictMode -Version Latest
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell 5.1 negotiates older TLS versions unless told otherwise, and
    # its progress bar makes a download several times slower.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $ProgressPreference = 'SilentlyContinue'

    if (-not $Version) { $Version = $env:RQX_VERSION }
    if (-not $InstallDir) { $InstallDir = $env:RQX_INSTALL_DIR }
    if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'rqx\bin' }

    $triple = Get-RqxTriple
    if (-not $Version) { $Version = Get-LatestRqxVersion }
    if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$') {
        throw "'$Version' is not a version like 1.2.3"
    }

    $archiveName = "rqx-$Version-$triple.tar.gz"
    $downloadBase = "$script:ReleasesUrl/download/v$Version"
    $archiveUrl = "$downloadBase/$archiveName"
    $checksumsUrl = "$downloadBase/SHA256SUMS"
    $target = Join-Path $InstallDir 'rqx.exe'

    if ($DryRun) {
        Write-Output 'rqx install, dry run: nothing will be downloaded or changed'
        Write-Step "version    $Version"
        Write-Step "platform   $triple"
        Write-Step "archive    $archiveUrl"
        Write-Step "checksums  $checksumsUrl"
        Write-Step "verify     $archiveName against SHA256SUMS before installing"
        Write-Step "install    $target (directory created if missing)"
        if ($AddToPath) { Write-Step "path       append $InstallDir to the user PATH" }
        return
    }

    $workDir = Join-Path ([IO.Path]::GetTempPath()) "rqx-install-$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $workDir | Out-Null
    try {
        Write-Output "Installing rqx $Version for $triple"
        $archivePath = Join-Path $workDir $archiveName
        $checksumsPath = Join-Path $workDir 'SHA256SUMS'

        Write-Step "downloading $archiveUrl"
        try {
            Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing
        } catch {
            Exit-WithFallback "could not download $archiveUrl (no binary published for v$Version on $triple, or the network failed)"
        }
        Write-Step "downloading $checksumsUrl"
        try {
            Invoke-WebRequest -Uri $checksumsUrl -OutFile $checksumsPath -UseBasicParsing
        } catch {
            Exit-WithFallback "could not download $checksumsUrl"
        }

        Write-Step 'verifying checksum'
        Assert-Checksum -ArchivePath $archivePath -ChecksumsPath $checksumsPath -ArchiveName $archiveName

        $extractDir = Join-Path $workDir 'extracted'
        New-Item -ItemType Directory -Path $extractDir | Out-Null
        & (Get-TarPath) -xzf $archivePath -C $extractDir rqx.exe
        if ($LASTEXITCODE -ne 0) {
            throw "$archiveName does not contain an rqx.exe binary; nothing was installed"
        }

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        Copy-Item -Path (Join-Path $extractDir 'rqx.exe') -Destination $target -Force
        Write-Step "installed $target"

        & $target --version
    } finally {
        Remove-Item -Path $workDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-PathAdvice -Directory $InstallDir -Append $AddToPath
}

Install-Rqx -Version $Version -InstallDir $InstallDir -DryRun $DryRun.IsPresent -AddToPath $AddToPath.IsPresent
