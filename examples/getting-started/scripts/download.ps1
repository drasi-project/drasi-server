# Drasi Server Install Script for Windows
# Downloads the Windows x64 binaries

$ErrorActionPreference = "Stop"

$RepoUrl = "https://github.com/drasi-project/drasi-server/releases/latest/download"
$InstallDir = "bin"

# Check architecture
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

if ($Arch -ne "X64") {
    Write-Host "Warning: Detected $Arch architecture. Only x64 binaries are available."
    Write-Host "The download will proceed with the x64 binary."
}

Write-Host "Detected: Windows ($Arch)"

# Create install directory
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Download drasi-server
$ServerBinary = "drasi-server-x86_64-windows.exe"
$ServerPath = Join-Path $InstallDir "drasi-server.exe"
Write-Host "Downloading: $ServerBinary"
try {
    Invoke-WebRequest -Uri "$RepoUrl/$ServerBinary" -OutFile $ServerPath -UseBasicParsing
} catch {
    Write-Host "Error: Failed to download $ServerBinary"
    Write-Host $_.Exception.Message
    exit 1
}

# Download drasi-sse-cli
$SseBinary = "drasi-sse-cli-x86_64-windows.exe"
$SsePath = Join-Path $InstallDir "drasi-sse-cli.exe"
Write-Host "Downloading: $SseBinary"
try {
    Invoke-WebRequest -Uri "$RepoUrl/$SseBinary" -OutFile $SsePath -UseBasicParsing
} catch {
    Write-Host "Error: Failed to download $SseBinary"
    Write-Host $_.Exception.Message
    exit 1
}

# Remove Windows security block on downloaded files
Unblock-File -Path $ServerPath
Unblock-File -Path $SsePath

# Verify
Write-Host ""
Write-Host "Verifying installations..."
try {
    & $ServerPath --version
    & $SsePath --version
    Write-Host ""
    Write-Host "✅ Drasi Server installed to $ServerPath"
    Write-Host "✅ Drasi SSE CLI installed to $SsePath"
} catch {
    Write-Host "Error: Failed to verify installation"
    Write-Host $_.Exception.Message
    exit 1
}
