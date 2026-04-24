# shikomi — Windows dev environment bootstrap (PowerShell 7+).
#
# 設計書: docs/features/dev-workflow/detailed-design.md §scripts/setup.ps1 のステップ契約
#
# 使い方:
#   pwsh scripts/setup.ps1
#   pwsh scripts/setup.ps1 -ToolsOnly  # CI が呼ぶ

[CmdletBinding()]
param(
    [switch]$ToolsOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# -------- step 0: PowerShell バージョン検査 (確定 A) -------------------
if ($PSVersionTable.PSVersion.Major -lt 7) {
    Write-Error "[FAIL] PowerShell 7 以上が必要です（検出: $($PSVersionTable.PSVersion)）。`n次のコマンド: winget install Microsoft.PowerShell"
    exit 1
}

# -------- ピン定数 (setup.sh と完全同期させること。audit-pin-sync.sh が CI で検証) -----
$LEFTHOOK_VERSION                 = "2.1.6"
$LEFTHOOK_SHA256_LINUX_X86_64     = "6704b01a72414affcc921740a7d6c621fe60c3082b291c9730900a2c6a352516"
$LEFTHOOK_SHA256_LINUX_ARM64      = "3fd749629968beb7f7f68cd0fc7b1b5ab801a1ec2045892586005cce75944118"
$LEFTHOOK_SHA256_MACOS_X86_64     = "93c6d51823f94a7f26a2bbb84f59504378b178f55d6c90744169693ed3e89013"
$LEFTHOOK_SHA256_MACOS_ARM64      = "f07c97c32376749edb5b34179c16c6d87dd3e7ca0040aee911f38c821de0daab"
$LEFTHOOK_SHA256_WINDOWS_X86_64   = "6704b01a72414affcc921740a7d6c621fe60c3082b291c9730900a2c6a352516"

$GITLEAKS_VERSION                 = "8.30.1"
$GITLEAKS_SHA256_LINUX_X86_64     = "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"
$GITLEAKS_SHA256_LINUX_ARM64      = "e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080"
$GITLEAKS_SHA256_MACOS_X86_64     = "dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709"
$GITLEAKS_SHA256_MACOS_ARM64      = "b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5"
$GITLEAKS_SHA256_WINDOWS_X86_64   = "d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e"

# 空値チェック (step 2)
$pinVars = @(
    'LEFTHOOK_VERSION','LEFTHOOK_SHA256_LINUX_X86_64','LEFTHOOK_SHA256_LINUX_ARM64',
    'LEFTHOOK_SHA256_MACOS_X86_64','LEFTHOOK_SHA256_MACOS_ARM64','LEFTHOOK_SHA256_WINDOWS_X86_64',
    'GITLEAKS_VERSION','GITLEAKS_SHA256_LINUX_X86_64','GITLEAKS_SHA256_LINUX_ARM64',
    'GITLEAKS_SHA256_MACOS_X86_64','GITLEAKS_SHA256_MACOS_ARM64','GITLEAKS_SHA256_WINDOWS_X86_64'
)
foreach ($v in $pinVars) {
    if ([string]::IsNullOrEmpty((Get-Variable -Name $v -ValueOnly))) {
        Write-Error "[FAIL] pin 定数 $v が空です。setup.ps1 の冒頭で値を確定してください。"
        exit 1
    }
}

# -------- step 3: cwd 検査 -------------------------------------------
if (-not (Test-Path .git)) {
    Write-Error "[FAIL] .git/ ディレクトリが見つかりません。リポジトリルートで実行してください。`n現在のディレクトリ: $(Get-Location)"
    exit 1
}

# -------- step 4: Rust toolchain 検査 --------------------------------
if (-not (Get-Command rustc -ErrorAction SilentlyContinue) -or -not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "[FAIL] Rust toolchain が未検出です。`n次のコマンド: https://rustup.rs/ の手順に従って rustup を導入してください。"
    exit 1
}

# Windows x86_64 固定 (PowerShell 7+ で pwsh 経由実行される前提)
$Platform = "windows_x86_64"

$BinDir = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME "bin" } else { Join-Path $env:USERPROFILE ".cargo\bin" }
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

# -------- cargo install helper (step 5 / 6) --------------------------
function Install-CargoTool {
    param([string]$Tool)
    if (Get-Command $Tool -ErrorAction SilentlyContinue) {
        $ver = try { & $Tool --version 2>$null | Select-Object -First 1 } catch { "unknown" }
        Write-Host "[SKIP] $Tool は既にインストール済みです。"
        Write-Host "バージョン: $ver"
        return
    }
    cargo install --locked $Tool
    if ($LASTEXITCODE -ne 0) {
        Write-Error "[FAIL] cargo install --locked $Tool に失敗しました。"
        exit 1
    }
}

Install-CargoTool just
Install-CargoTool convco

# -------- SHA256 helper ----------------------------------------------
function Get-Sha256 {
    param([string]$Path)
    (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLower()
}

function Invoke-PinnedDownload {
    param(
        [string]$Url,
        [string]$ExpectedSha,
        [string]$OutFile,
        [string]$ToolLabel
    )
    Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
    $actual = Get-Sha256 -Path $OutFile
    if ($actual -ne $ExpectedSha.ToLower()) {
        Remove-Item -Force $OutFile -ErrorAction SilentlyContinue
        Write-Error "[FAIL] $ToolLabel バイナリの SHA256 検証に失敗しました。サプライチェーン改ざんの可能性があります。`n次のコマンド: 一時ファイルを削除後にネットワーク状況を確認し再実行。繰り返し失敗する場合は Issue で報告してください。"
        exit 1
    }
}

# -------- step 7: lefthook --------------------------------------------
function Install-Lefthook {
    if (Get-Command lefthook -ErrorAction SilentlyContinue) {
        $ver = try { lefthook version 2>$null } catch { "unknown" }
        Write-Host "[SKIP] lefthook は既にインストール済みです。"
        Write-Host "バージョン: $ver"
        return
    }
    $asset    = "lefthook_${LEFTHOOK_VERSION}_Windows_x86_64.gz"
    $url      = "https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/$asset"
    $tmp      = New-TemporaryFile
    $tmpGz    = "$tmp.gz"
    Move-Item -Force $tmp $tmpGz
    Invoke-PinnedDownload -Url $url -ExpectedSha $LEFTHOOK_SHA256_WINDOWS_X86_64 -OutFile $tmpGz -ToolLabel "lefthook"
    # gunzip 相当: System.IO.Compression.GzipStream でバイナリを展開
    $dst = Join-Path $BinDir "lefthook.exe"
    $in  = [System.IO.File]::OpenRead($tmpGz)
    try {
        $gz  = New-Object System.IO.Compression.GzipStream($in, [System.IO.Compression.CompressionMode]::Decompress)
        try {
            $out = [System.IO.File]::Create($dst)
            try   { $gz.CopyTo($out) }
            finally { $out.Dispose() }
        } finally { $gz.Dispose() }
    } finally { $in.Dispose() }
    Remove-Item -Force $tmpGz
}

function Install-Gitleaks {
    if (Get-Command gitleaks -ErrorAction SilentlyContinue) {
        $ver = try { gitleaks version 2>$null } catch { "unknown" }
        Write-Host "[SKIP] gitleaks は既にインストール済みです。"
        Write-Host "バージョン: $ver"
        return
    }
    $asset = "gitleaks_${GITLEAKS_VERSION}_windows_x64.zip"
    $url   = "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/$asset"
    $tmpDir = New-Item -ItemType Directory -Path ([System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), [System.Guid]::NewGuid().ToString())) -Force
    $tmpZip = Join-Path $tmpDir $asset
    Invoke-PinnedDownload -Url $url -ExpectedSha $GITLEAKS_SHA256_WINDOWS_X86_64 -OutFile $tmpZip -ToolLabel "gitleaks"
    Expand-Archive -Path $tmpZip -DestinationPath $tmpDir -Force
    Move-Item -Force (Join-Path $tmpDir "gitleaks.exe") (Join-Path $BinDir "gitleaks.exe")
    Remove-Item -Recurse -Force $tmpDir
}

Install-Lefthook
Install-Gitleaks

# -------- step 9: lefthook install -----------------------------------
if (-not $ToolsOnly) {
    lefthook install
    if ($LASTEXITCODE -ne 0) {
        Write-Error "[FAIL] lefthook install に失敗しました。"
        exit 1
    }
}

# -------- step 10: 完了ログ ------------------------------------------
Write-Host "[OK] Setup complete. Git フックが有効化されました。"
