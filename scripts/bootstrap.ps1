# 一键安装 binox 到 %LOCALAPPDATA%\binox\bin（或 BINOX_INSTALL_DIR）
#
#   irm https://github.com/spraylee/binox/releases/latest/download/bootstrap.ps1 | iex
#
# 环境变量：BINOX_REPOSITORY / BINOX_VERSION / BINOX_INSTALL_DIR / BINOX_RELEASE_BASE_URL

$ErrorActionPreference = "Stop"

$Repository = if ($env:BINOX_REPOSITORY) { $env:BINOX_REPOSITORY } else { "spraylee/binox" }
$Version = if ($env:BINOX_VERSION) { $env:BINOX_VERSION } else { "" }
$InstallDir = if ($env:BINOX_INSTALL_DIR) {
    $env:BINOX_INSTALL_DIR
} elseif ($env:LOCALAPPDATA) {
    Join-Path $env:LOCALAPPDATA "binox\bin"
} else {
    Join-Path $HOME ".local\bin"
}

function Fail([string] $Message) {
    Write-Error "[binox] bootstrap: $Message"
    exit 1
}

if (-not $Version) {
    $probe = curl.exe -fsSI "https://github.com/$Repository/releases/latest"
    if ($LASTEXITCODE -ne 0) { Fail "无法探测 latest Release" }
    $location = ($probe | Where-Object { $_ -match '^Location:' } | Select-Object -First 1)
    if ($location -match '/releases/tag/([^/\s]+)') {
        $Version = $Matches[1].Trim()
    }
}
if (-not $Version) { Fail "无法确定版本，请设置 BINOX_VERSION" }

$architecture = if ($env:PROCESSOR_ARCHITECTURE) {
    $env:PROCESSOR_ARCHITECTURE.ToUpperInvariant()
} else {
    "AMD64"
}
$Target = switch ($architecture) {
    "AMD64" { "x86_64-pc-windows-msvc"; break }
    default { Fail "不支持的 Windows 架构: $architecture（目前只发 x64）" }
}

$Ver = $Version.TrimStart('v')
$AssetZip = "binox-v$Ver-$Target.zip"
$AssetTar = "binox-v$Ver-$Target.tar.gz"
$ReleaseBaseUrl = if ($env:BINOX_RELEASE_BASE_URL) {
    $env:BINOX_RELEASE_BASE_URL.TrimEnd('/')
} else {
    "https://github.com/$Repository/releases/download/$Version"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("binox-bootstrap-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    Write-Host "[binox] 下载 $Version ($Target)"
    $sumsPath = Join-Path $tmp "SHA256SUMS"
    & curl.exe --fail --location --retry 3 --retry-delay 1 --output $sumsPath "$ReleaseBaseUrl/SHA256SUMS"
    if ($LASTEXITCODE -ne 0) { Fail "下载 SHA256SUMS 失败" }

    $asset = $AssetZip
    $archive = Join-Path $tmp $asset
    & curl.exe --fail --location --retry 3 --retry-delay 1 --output $archive "$ReleaseBaseUrl/$asset"
    if ($LASTEXITCODE -ne 0) {
        $asset = $AssetTar
        $archive = Join-Path $tmp $asset
        & curl.exe --fail --location --retry 3 --retry-delay 1 --output $archive "$ReleaseBaseUrl/$asset"
        if ($LASTEXITCODE -ne 0) { Fail "下载失败: $AssetZip / $AssetTar" }
    }

    $expected = $null
    Get-Content -LiteralPath $sumsPath | ForEach-Object {
        $parts = $_ -split '\s+', 2
        if ($parts.Count -ge 2) {
            $name = $parts[1].TrimStart('*')
            if ($name -eq $asset) { $expected = $parts[0].ToLowerInvariant() }
        }
    }
    if (-not $expected) { Fail "SHA256SUMS 中没有 $asset" }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { Fail "SHA256 校验失败: $asset" }

    $extracted = Join-Path $tmp "extracted"
    New-Item -ItemType Directory -Force -Path $extracted | Out-Null
    if ($asset.EndsWith(".zip")) {
        Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force
    } else {
        Fail "本脚本在 Windows 上优先使用 .zip 资产"
    }

    $bin = Get-ChildItem -Path $extracted -Recurse -File -Filter "binox.exe" | Select-Object -First 1
    if (-not $bin) { Fail "归档内没有 binox.exe" }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $dest = Join-Path $InstallDir "binox.exe"
    Copy-Item -LiteralPath $bin.FullName -Destination "$dest.new" -Force
    Move-Item -LiteralPath "$dest.new" -Destination $dest -Force
    Write-Host "[binox] 已安装到 $dest"
    if ($env:PATH -notlike "*$InstallDir*") {
        Write-Host "[binox] 请把 $InstallDir 加入 PATH"
    }
    & $dest --version
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
