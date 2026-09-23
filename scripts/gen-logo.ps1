<#
.SYNOPSIS
  Genera los assets del logo de Winarchy a partir de los poligonos de la marca.
.DESCRIPTION
  Fuentes: assets/logo/winarchy-mark.svg (marca) y assets/logo/winarchy-icon.svg (icono).
  Emite:
    - assets/logo/winarchy-logo.svg          (copia de la marca, para el README)
    - assets/logo/winarchy-logo.png          (512x512, el icono)
    - assets/logo/winarchy-social.png        (1280x640, social preview de GitHub)
    - assets/logo/winarchy.ico               (16/32/48/256, tray icon del stack)
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$outDir = Join-Path $root 'assets\logo'
$markSvg = Join-Path $outDir 'winarchy-mark.svg'
$iconSvg = Join-Path $outDir 'winarchy-icon.svg'

Add-Type -AssemblyName System.Drawing

# Poligonos (con fill heredado y translate acumulado) y rect de fondo de un SVG plano.
function Read-SvgShapes {
    param([string]$Path)
    $svg = ([xml](Get-Content $Path -Raw)).DocumentElement
    $vb = $svg.viewBox -split '[ ,]+' | ForEach-Object { [double]$_ }
    $shapes = [System.Collections.Generic.List[object]]::new()
    $walk = {
        param($node, [string]$fill, [double]$dx, [double]$dy)
        foreach ($n in $node.ChildNodes) {
            if ($n.NodeType -ne 'Element') { continue }
            $f = if ($n.fill) { $n.fill } else { $fill }
            $x = $dx; $y = $dy
            if ($n.transform -match 'translate\(\s*([-\d.]+)[ ,]+([-\d.]+)\s*\)') { $x += [double]$Matches[1]; $y += [double]$Matches[2] }
            switch ($n.LocalName) {
                'rect' { $shapes.Add([pscustomobject]@{ Kind = 'rect'; Fill = $f; W = [double]$n.width; H = [double]$n.height; Rx = [double]$n.rx }) }
                'polygon' {
                    $pts = foreach ($p in ($n.points.Trim() -split '\s+')) {
                        $xy = $p -split ','
                        [System.Drawing.PointF]::new([double]$xy[0] + $x, [double]$xy[1] + $y)
                    }
                    $shapes.Add([pscustomobject]@{ Kind = 'polygon'; Fill = $f; Points = $pts })
                }
                'g' { & $walk $n $f $x $y }
            }
        }
    }
    & $walk $svg '#000000' 0 0
    [pscustomobject]@{ Width = $vb[2]; Height = $vb[3]; Shapes = $shapes }
}

function Draw-Shapes {
    param([System.Drawing.Graphics]$G, $Svg, [double]$Scale, [double]$OffX, [double]$OffY)
    foreach ($s in $Svg.Shapes) {
        $brush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml($s.Fill))
        if ($s.Kind -eq 'rect') {
            $w = $s.W * $Scale; $h = $s.H * $Scale; $d = 2 * $s.Rx * $Scale
            $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
            $path.AddArc($OffX, $OffY, $d, $d, 180, 90)
            $path.AddArc($OffX + $w - $d, $OffY, $d, $d, 270, 90)
            $path.AddArc($OffX + $w - $d, $OffY + $h - $d, $d, $d, 0, 90)
            $path.AddArc($OffX, $OffY + $h - $d, $d, $d, 90, 90)
            $path.CloseFigure()
            $G.FillPath($brush, $path)
            $path.Dispose()
        } else {
            $pts = [System.Drawing.PointF[]]@($s.Points | ForEach-Object {
                    [System.Drawing.PointF]::new($OffX + $_.X * $Scale, $OffY + $_.Y * $Scale) })
            $G.FillPolygon($brush, $pts)
        }
        $brush.Dispose()
    }
}

function New-Canvas {
    param([int]$W, [int]$H)
    $bmp = [System.Drawing.Bitmap]::new($W, $H)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $bmp, $g
}

function New-IconBitmap {
    param([int]$Size)
    $bmp, $g = New-Canvas $Size $Size
    Draw-Shapes $g $icon ($Size / $icon.Width) 0 0
    $g.Dispose()
    $bmp
}

$mark = Read-SvgShapes $markSvg
$icon = Read-SvgShapes $iconSvg
$ink = ($icon.Shapes | Where-Object Kind -EQ 'rect').Fill
$paper = ($mark.Shapes | Select-Object -First 1).Fill

# --- SVG (README) ---
$svgPath = Join-Path $outDir 'winarchy-logo.svg'
Copy-Item $markSvg $svgPath -Force
Write-Host "OK: $svgPath"

# --- PNG cuadrado ---
$pngPath = Join-Path $outDir 'winarchy-logo.png'
$bmp = New-IconBitmap 512
$bmp.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "OK: $pngPath"

# --- Social preview: marca a la izquierda + "winarchy" en mono bold ---
$socialPath = Join-Path $outDir 'winarchy-social.png'
$fontName = [System.Drawing.Text.InstalledFontCollection]::new().Families |
    Where-Object { $_.Name -in 'JetBrains Mono', 'Cascadia Mono', 'Consolas' -and $_.IsStyleAvailable([System.Drawing.FontStyle]::Bold) } |
    Sort-Object { @('JetBrains Mono', 'Cascadia Mono', 'Consolas').IndexOf($_.Name) } |
    Select-Object -First 1 -ExpandProperty Name
$bmp, $g = New-Canvas 1280 640
$g.Clear([System.Drawing.ColorTranslator]::FromHtml($ink))
$markH = 180.0
$scale = $markH / $mark.Height
$markW = $mark.Width * $scale
if ($fontName) {
    $font = [System.Drawing.Font]::new($fontName, 128, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
    $fmt = [System.Drawing.StringFormat]::GenericTypographic
    $text = $g.MeasureString('winarchy', $font, [System.Drawing.PointF]::Empty, $fmt)
    $gap = 72
    $x = (1280 - ($markW + $gap + $text.Width)) / 2
    Draw-Shapes $g $mark $scale $x ((640 - $markH) / 2)
    # baseline de la palabra alineada con la base de la marca
    $ascent = $font.Size * $font.FontFamily.GetCellAscent($font.Style) / $font.FontFamily.GetEmHeight($font.Style)
    $textBrush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml($paper))
    $g.DrawString('winarchy', $font, $textBrush, [float]($x + $markW + $gap), [float]((640 + $markH) / 2 - $ascent), $fmt)
    $textBrush.Dispose(); $font.Dispose()
    Write-Host "Fuente: $fontName"
} else {
    Draw-Shapes $g $mark $scale ((1280 - $markW) / 2) ((640 - $markH) / 2)
    Write-Warning 'Sin fuente mono bold instalada: social preview solo con la marca.'
}
$g.Dispose()
$bmp.Save($socialPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "OK: $socialPath"

# --- ICO (tray icon de Winarchy), frames PNG ---
function New-LogoIco {
    param([int[]]$Sizes, [string]$Path)
    $frames = foreach ($s in $Sizes) {
        $bmp = New-IconBitmap $s
        $ms = New-Object System.IO.MemoryStream
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        [pscustomobject]@{ Size = $s; Bytes = $ms.ToArray() }
    }
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create)
    $bw = New-Object System.IO.BinaryWriter($fs)
    # ICONDIR: reserved(0), type(1=icon), count
    $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$frames.Count)
    # offset al primer frame: 6 (header) + 16 * count (directorio)
    $offset = 6 + 16 * $frames.Count
    foreach ($f in $frames) {
        $dim = if ($f.Size -ge 256) { 0 } else { $f.Size }   # 0 == 256 en el formato ICO
        $bw.Write([byte]$dim)            # width
        $bw.Write([byte]$dim)            # height
        $bw.Write([byte]0)               # color count (0 = >=256)
        $bw.Write([byte]0)               # reserved
        $bw.Write([uint16]1)             # color planes
        $bw.Write([uint16]32)            # bits per pixel
        $bw.Write([uint32]$f.Bytes.Length)
        $bw.Write([uint32]$offset)
        $offset += $f.Bytes.Length
    }
    foreach ($f in $frames) { $bw.Write($f.Bytes) }
    $bw.Flush(); $bw.Dispose(); $fs.Dispose()
    Write-Host "OK: $Path"
}
New-LogoIco -Sizes @(16, 32, 48, 256) -Path (Join-Path $outDir 'winarchy.ico')
