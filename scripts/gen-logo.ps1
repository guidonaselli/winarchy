<#
.SYNOPSIS
  Genera los assets del logo de Winarchy a partir de config/fastfetch/logo.txt.
.DESCRIPTION
  Parsea la grilla ASCII (resolviendo los codigos de color $1/$2 de fastfetch) y
  emite:
    - assets/logo/winarchy-logo.svg          (vectorial, para el README)
    - assets/logo/winarchy-social.png        (1280x640, social preview de GitHub)
    - assets/logo/winarchy-logo.png          (512x512, asset cuadrado / favicon)
  Dos tonos fijos: morado + verde, gaps en negro tokyo-night.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$logoTxt = Join-Path $root 'config\fastfetch\logo.txt'
$outDir = Join-Path $root 'assets\logo'
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir -Force | Out-Null }

# --- Colores de marca (dos tonos fijos) ---
$PURPLE = @(157, 124, 216)   # #9d7cd8
$GREEN  = @(158, 206, 106)   # #9ece6a
$BG     = @(26, 27, 38)      # #1a1b26 (tokyo-night)

# --- Parseo de la grilla ---
$lines = Get-Content $logoTxt -Encoding UTF8
$grid = [System.Collections.Generic.List[char[]]]::new()
foreach ($line in $lines) {
    $row = [System.Collections.Generic.List[char]]::new()
    $color = 'p'
    $i = 0
    while ($i -lt $line.Length) {
        $ch = $line[$i]
        if ($ch -eq '$' -and ($i + 1) -lt $line.Length) {
            $n = $line[$i + 1]
            if ($n -eq '1') { $color = 'p'; $i += 2; continue }
            if ($n -eq '2') { $color = 'g'; $i += 2; continue }
        }
        if ([int][char]$ch -eq 0x2588) { $row.Add($color) } else { $row.Add('.') }
        $i++
    }
    $grid.Add($row.ToArray())
}
$gw = ($grid | ForEach-Object { $_.Length } | Measure-Object -Maximum).Maximum
$gh = $grid.Count
# normalizar ancho
$cells = for ($y = 0; $y -lt $gh; $y++) {
    $r = $grid[$y]
    , @(for ($x = 0; $x -lt $gw; $x++) { if ($x -lt $r.Length) { $r[$x] } else { '.' } })
}
Write-Host "Grilla: ${gw}x${gh}"

# Las celdas del terminal son ~1:2 (ancho:alto): para replicar la proporcion del
# logo tal como lo dibuja fastfetch, cada celda se renderiza al doble de alta.
$YSCALE = 2

# --- SVG (README) ---
$svgH = $gh * $YSCALE
$svg = [System.Text.StringBuilder]::new()
[void]$svg.AppendLine("<svg xmlns=`"http://www.w3.org/2000/svg`" viewBox=`"0 0 $gw $svgH`" shape-rendering=`"crispEdges`">")
[void]$svg.AppendLine("  <rect width=`"$gw`" height=`"$svgH`" fill=`"#1a1b26`"/>")
for ($y = 0; $y -lt $gh; $y++) {
    for ($x = 0; $x -lt $gw; $x++) {
        $c = $cells[$y][$x]
        if ($c -eq '.') { continue }
        $fill = if ($c -eq 'p') { '#9d7cd8' } else { '#9ece6a' }
        [void]$svg.AppendLine("  <rect x=`"$x`" y=`"$($y * $YSCALE)`" width=`"1`" height=`"$YSCALE`" fill=`"$fill`"/>")
    }
}
[void]$svg.AppendLine('</svg>')
$svgPath = Join-Path $outDir 'winarchy-logo.svg'
Set-Content -Path $svgPath -Value $svg.ToString() -Encoding UTF8
Write-Host "OK: $svgPath"

# --- PNGs via System.Drawing ---
Add-Type -AssemblyName System.Drawing
function New-LogoPng {
    param([int]$CanvasW, [int]$CanvasH, [double]$Fill, [string]$Path)
    $bmp = New-Object System.Drawing.Bitmap($CanvasW, $CanvasH)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::None
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
    $bgBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb($BG[0], $BG[1], $BG[2]))
    $g.FillRectangle($bgBrush, 0, 0, $CanvasW, $CanvasH)
    # cell ancho; alto = cell * YSCALE (proporcion del terminal)
    $cell = [Math]::Floor([Math]::Min(($CanvasW * $Fill) / $gw, ($CanvasH * $Fill) / ($gh * $YSCALE)))
    $cellH = $cell * $YSCALE
    $logoW = $cell * $gw; $logoH = $cellH * $gh
    $offX = [int](($CanvasW - $logoW) / 2); $offY = [int](($CanvasH - $logoH) / 2)
    $purpleBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb($PURPLE[0], $PURPLE[1], $PURPLE[2]))
    $greenBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb($GREEN[0], $GREEN[1], $GREEN[2]))
    for ($y = 0; $y -lt $gh; $y++) {
        for ($x = 0; $x -lt $gw; $x++) {
            $c = $cells[$y][$x]
            if ($c -eq '.') { continue }
            $brush = if ($c -eq 'p') { $purpleBrush } else { $greenBrush }
            $g.FillRectangle($brush, $offX + $x * $cell, $offY + $y * $cellH, $cell, $cellH)
        }
    }
    $g.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "OK: $Path"
}
New-LogoPng -CanvasW 1280 -CanvasH 640 -Fill 0.9 -Path (Join-Path $outDir 'winarchy-social.png')
New-LogoPng -CanvasW 512  -CanvasH 512 -Fill 0.85 -Path (Join-Path $outDir 'winarchy-logo.png')
