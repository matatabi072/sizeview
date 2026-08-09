#requires -Version 5.1
<#
.SYNOPSIS
  配布用の release ビルドを作り、ビルド機の情報が埋まっていないか検査する。

.DESCRIPTION
  Rust は panic の位置情報として、依存クレートの**絶対パス**を実行ファイルに埋め込む。
  何もしないと `C:\Users\<利用者名>\.cargo\registry\...` が数百か所入り、
  配布した時点で利用者名が漏れる (2026-08-10 の実測では 575 か所)。

  `--remap-path-prefix` で置き換えてから、実際にバイナリを走査して
  絶対パスが残っていないことを確かめる。**検査に落ちたら 0 以外で終了する**ので、
  そのまま配布してしまう事故を防げる。

  Cargo の `trim-paths` が安定したら、この置き換え部分は不要になる。

.NOTES
  このスクリプト自身は UTF-8 BOM 付きで保存すること (PowerShell 5.1 対策)。
#>
param(
    [switch]$SkipBuild,
    # 検査だけ別のバイナリに対して行いたいとき (debug 版の確認など)
    [string]$ExePath
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

# 実行ファイル名は cargo に聞く。プロジェクトごとに書き換えると、
# 別のプロジェクトへ持っていったときに古い名前のまま「見つからない」で止まる。
function Get-BinName {
    Push-Location $root
    try {
        $meta = (cargo metadata --no-deps --format-version 1 2>$null) | ConvertFrom-Json
        $names = @()
        foreach ($p in $meta.packages) {
            foreach ($t in $p.targets) { if ($t.kind -contains "bin") { $names += $t.name } }
        }
        if ($names.Count -eq 0) { throw "bin ターゲットが見つかりません" }
        return $names[0]
    }
    finally { Pop-Location }
}

if ($ExePath) {
    $exe = $ExePath
    $SkipBuild = $true
} else {
    $exe = Join-Path $root ("target\release\{0}.exe" -f (Get-BinName))
}

if (-not $SkipBuild) {
    # 置き換え元は環境から組み立てる (ここに個人のパスを書かない)
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
    $rustup    = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }

    $flags = @(
        "--remap-path-prefix=$cargoHome=/cargo"
        "--remap-path-prefix=$rustup=/rustup"
        "--remap-path-prefix=$root=/src"
        "--remap-path-prefix=$env:USERPROFILE=/home"
    )
    $env:RUSTFLAGS = ($flags -join " ")
    Write-Host "RUSTFLAGS = $env:RUSTFLAGS"

    Push-Location $root
    try {
        if (Test-Path $exe) { Remove-Item -LiteralPath $exe -Force }
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build --release に失敗しました" }
    }
    finally {
        Pop-Location
        Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
    }
}

if (-not (Test-Path $exe)) { throw "実行ファイルがありません: $exe" }

Write-Host ""
Write-Host "=== 配布前の検査 ==="
$bytes = [System.IO.File]::ReadAllBytes($exe)
$text = [System.Text.Encoding]::ASCII.GetString($bytes)

# 利用者名そのものと、Windows の絶対パスの形を探す。
# ★「C:\\Users」のように書くと PowerShell では二重バックスラッシュの literal になり、
#   何も見つからずに素通りする。ここは 1 本で書くこと。
$userName = Split-Path $env:USERPROFILE -Leaf

# 見逃さないことと同じくらい、**空振りしないこと**が大事。
# 毎回 NG が出る検査は、そのうち誰も読まなくなる。
# `C:\Windows\...` は自前で持っているフォントのパスなので、機械の素性を明かさない。
$allowed = @("C:\Windows\")

$patterns = [ordered]@{
    "利用者名 ($userName)"   = [regex]::Escape($userName)
    "利用者フォルダー"       = "Users\\[A-Za-z0-9_.\-]{2,}"
    # 2 段以上の実在しそうな形だけを見る (バイナリの偶然の並びを拾わないため)
    "ビルド機の絶対パス"     = "[A-Za-z]:\\[A-Za-z0-9_.\-]{2,}\\[A-Za-z0-9_.\-]{2,}"
}

$bad = 0
foreach ($name in $patterns.Keys) {
    $found = [regex]::Matches($text, $patterns[$name]) |
        ForEach-Object { $_.Value } |
        Where-Object { $v = $_; -not ($allowed | Where-Object { $v.StartsWith($_) }) } |
        Select-Object -Unique
    $count = @($found).Count
    $mark = if ($count -eq 0) { "OK  " } else { "NG  " }
    "{0}{1,-24} {2} 件" -f $mark, $name, $count
    if ($count -gt 0) {
        $bad += $count
        $found | Select-Object -First 5 | ForEach-Object { "      $_" }
    }
}

$size = [math]::Round((Get-Item $exe).Length / 1MB, 1)
Write-Host ""
Write-Host ("実行ファイル: {0} ({1} MB)" -f $exe, $size)
if ($bad -gt 0) {
    Write-Error "ビルド機の情報が埋まっています。このまま配布しないこと。"
    exit 1
}
Write-Host "検査に通りました。配布して問題ありません。"
