# Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
# Licenca GNU GPL 3 ou posterior.
#
# windows.ps1 - o pacote do Windows: o HexagonCall.exe (release), o
# instalador HexagonCallSetup-X.Y.Z.exe (Inno Setup, pacote/instalador.iss) e
# o SHA256SUMS.txt que o atualizador do app confere antes de instalar.
#
#   pwsh pacote/windows.ps1                 compila e empacota
#   pwsh pacote/windows.ps1 -SemCompilar    so empacota o que ja esta em target
#   pwsh pacote/windows.ps1 -Versao 0.1.1   outra versao no instalador (teste)
#
# Tudo vai para pacote/saida (fora do git). Precisa do Inno Setup 6 (ISCC.exe
# no PATH ou na pasta padrao; o CI instala pelo choco).
#
# Padrao de publicacao (o mesmo do Acessos): os anexos da release sao
# HexagonCallSetup-X.Y.Z.exe, HexagonCall.exe e SHA256SUMS.txt, com ESTE nome.
# O atualizador acha o instalador pelo prefixo e o hash pela linha do
# SHA256SUMS: sem as duas coisas, a release nao e oferecida.
param(
    [switch]$SemCompilar,
    [string]$Versao
)
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
$saida = Join-Path $PSScriptRoot 'saida'

if (-not $Versao) {
    # A do [workspace.package] do Cargo.toml: a mesma do executavel.
    $m = Select-String -Path (Join-Path $raiz 'Cargo.toml') -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if (-not $m) { throw 'nao achei a versao no Cargo.toml' }
    $Versao = $m.Matches[0].Groups[1].Value
}
Write-Host ">> Hexagon Call $Versao"

if (-not $SemCompilar) {
    Push-Location $raiz
    try {
        cargo build --release --locked -p janela
        if ($LASTEXITCODE) { throw "cargo build falhou ($LASTEXITCODE)" }
    } finally { Pop-Location }
}

New-Item -ItemType Directory -Force $saida | Out-Null
Get-ChildItem $saida -File | Remove-Item -Force
Copy-Item (Join-Path $raiz 'target\release\janela.exe') (Join-Path $saida 'HexagonCall.exe') -Force

# O icone da tela de espera do instalador: BMP de 24 bits (o TBitmapImage do
# Inno nao le alfa), achatado sobre o branco do cartao da tela.
Add-Type -AssemblyName System.Drawing
$png = [System.Drawing.Image]::FromFile((Join-Path $raiz 'janela\icons\icon.png'))
try {
    $bmp = New-Object System.Drawing.Bitmap 192, 192, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::White)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.DrawImage($png, 0, 0, 192, 192)
    $g.Dispose()
    $bmp.Save((Join-Path $saida 'espera-icone.bmp'), [System.Drawing.Imaging.ImageFormat]::Bmp)
    $bmp.Dispose()
} finally { $png.Dispose() }

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $iscc = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) nao encontrado' }
Write-Host ">> compilando o instalador ($iscc)"
& $iscc /Q "/DAppVersion=$Versao" "/DSaida=$saida" "/O$saida" (Join-Path $PSScriptRoot 'instalador.iss')
if ($LASTEXITCODE) { throw "ISCC falhou ($LASTEXITCODE)" }
Remove-Item (Join-Path $saida 'espera-icone.bmp') -Force

# SHA256SUMS.txt no formato do sha256sum ("hash  nome"), com LF.
$linhas = foreach ($n in "HexagonCallSetup-$Versao.exe", 'HexagonCall.exe') {
    '{0}  {1}' -f (Get-FileHash (Join-Path $saida $n) -Algorithm SHA256).Hash.ToLowerInvariant(), $n
}
[System.IO.File]::WriteAllText((Join-Path $saida 'SHA256SUMS.txt'), (($linhas -join "`n") + "`n"))
Get-Content (Join-Path $saida 'SHA256SUMS.txt')
Get-ChildItem $saida | Select-Object Name, Length | Format-Table -AutoSize
