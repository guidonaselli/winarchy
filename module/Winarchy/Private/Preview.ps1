# Preview.ps1 — synthetic theme preview cards rendered from theme.toml (System.Drawing)

function ConvertTo-WinarchyDrawingColor {
    param([Parameter(Mandatory)][string]$Hex)
    [System.Drawing.ColorTranslator]::FromHtml($Hex)
}

function New-WinarchyThemePreview {
    <#
      Renders themes/<name>/preview.png as a synthetic palette card:
      background + fake terminal sample + 16 ANSI swatches + accent bar.
      Deterministic, no screenshots involved.
    #>
    param([Parameter(Mandatory)][string]$Name)
    Add-Type -AssemblyName System.Drawing

    $themeDir = Join-Path (Get-WinarchyThemesDir) $Name
    $theme = Import-WinarchyToml -Path (Join-Path $themeDir 'theme.toml')
    $c = $theme['colors']

    $W = 900; $H = 506
    $bg      = ConvertTo-WinarchyDrawingColor $c['background']
    $fg      = ConvertTo-WinarchyDrawingColor $c['foreground']
    $accent  = ConvertTo-WinarchyDrawingColor $c['accent']
    $selBg   = ConvertTo-WinarchyDrawingColor $c['selection_background']
    $dim     = ConvertTo-WinarchyDrawingColor $c['color8']

    $bmp = [System.Drawing.Bitmap]::new($W, $H)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.SmoothingMode = 'AntiAlias'
        $g.TextRenderingHint = 'ClearTypeGridFit'
        $g.Clear($bg)

        $fontTitle = [System.Drawing.Font]::new('Segoe UI', 22, [System.Drawing.FontStyle]::Bold)
        $fontMode  = [System.Drawing.Font]::new('Segoe UI', 11)
        $fontMono  = [System.Drawing.Font]::new('Consolas', 12)

        $bAccent = [System.Drawing.SolidBrush]::new($accent)
        $bFg     = [System.Drawing.SolidBrush]::new($fg)
        $bDim    = [System.Drawing.SolidBrush]::new($dim)

        # Accent bar on the left edge
        $g.FillRectangle($bAccent, 0, 0, 10, $H)

        # Header: theme name + mode
        $g.DrawString($theme['name'], $fontTitle, $bFg, 36, 26)
        $g.DrawString("$($theme['mode']) · $($c['accent'])", $fontMode, $bDim, 38, 68)

        # Fake terminal panel
        $panel = [System.Drawing.Rectangle]::new(36, 104, ($W - 72), 232)
        $panelBg = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255,
            [int][Math]::Min(255, $bg.R * 0.85 + 8), [int][Math]::Min(255, $bg.G * 0.85 + 8), [int][Math]::Min(255, $bg.B * 0.85 + 8)))
        $g.FillRectangle($panelBg, $panel)
        $g.DrawRectangle([System.Drawing.Pen]::new($selBg, 1), $panel)

        $lines = @(
            @(@($accent, '~/winarchy ❯ '), @($fg, 'winarchy theme set ' + $Name)),
            @(@((ConvertTo-WinarchyDrawingColor $c['color2']), '  [OK] '), @($fg, "Theme '$($theme['name'])' applied across all surfaces.")),
            @(@((ConvertTo-WinarchyDrawingColor $c['color3']), '  warn '), @($bDim.Color, 'sample warning line for palette contrast')),
            @(@((ConvertTo-WinarchyDrawingColor $c['color1']), '  err  '), @($bDim.Color, 'sample error line for palette contrast')),
            @(@((ConvertTo-WinarchyDrawingColor $c['color4']), '  func '), @((ConvertTo-WinarchyDrawingColor $c['color5']), 'render('), @((ConvertTo-WinarchyDrawingColor $c['color6']), '"preview"'), @((ConvertTo-WinarchyDrawingColor $c['color5']), ')')),
            @(@($accent, '~/winarchy ❯ '), @($fg, '▍'))
        )
        # GenericTypographic + MeasureTrailingSpaces: keeps trailing spaces between segments
        $sf = [System.Drawing.StringFormat]::new([System.Drawing.StringFormat]::GenericTypographic)
        $sf.FormatFlags = $sf.FormatFlags -bor [System.Drawing.StringFormatFlags]::MeasureTrailingSpaces
        $y = $panel.Y + 16
        foreach ($line in $lines) {
            $x = [float]($panel.X + 16)
            foreach ($seg in $line) {
                $col = if ($seg[0] -is [System.Drawing.SolidBrush]) { $seg[0].Color } else { $seg[0] }
                $brush = [System.Drawing.SolidBrush]::new($col)
                $text = [string]$seg[1]
                $g.DrawString($text, $fontMono, $brush, $x, $y, $sf)
                $x += $g.MeasureString($text, $fontMono, [int]$panel.Width, $sf).Width
                $brush.Dispose()
            }
            $y += 32
        }
        $sf.Dispose()

        # 16 ANSI swatches in two rows of 8
        $sw = 96; $sh = 44; $gap = 8; $x0 = 36; $y0 = 368
        for ($i = 0; $i -lt 16; $i++) {
            $col = ConvertTo-WinarchyDrawingColor $c["color$i"]
            $x = $x0 + ($i % 8) * ($sw + $gap)
            $yy = $y0 + [int][Math]::Floor($i / 8) * ($sh + $gap)
            $b = [System.Drawing.SolidBrush]::new($col)
            $g.FillRectangle($b, $x, $yy, $sw, $sh)
            $b.Dispose()
        }

        $out = Join-Path $themeDir 'preview.png'
        $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
        $out
    }
    finally {
        $g.Dispose(); $bmp.Dispose()
    }
}

function Update-WinarchyThemePreviews {
    <# Regenerates preview.png for every installed theme (or one with -Name). #>
    param([string]$Name)
    $targets = if ($Name) { @($Name) } else { Get-WinarchyThemes }
    foreach ($t in $targets) {
        try {
            $out = New-WinarchyThemePreview -Name $t
            Write-WinarchyOk "Preview rendered: $out"
        }
        catch {
            Write-WinarchyWarn "Preview failed for '$t': $($_.Exception.Message)"
        }
    }
}
