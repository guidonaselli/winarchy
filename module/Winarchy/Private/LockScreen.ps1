$script:MarkBars = @(
    @(@(0, 0), @(14, 0), @(25, 56), @(11, 56)),
    @(@(28.5, 56), @(42.5, 56), @(50.2, 16.8), @(36.2, 16.8)),
    @(@(53.7, 16.8), @(67.7, 16.8), @(75.4, 56), @(61.4, 56)),
    @(@(78.9, 56), @(92.9, 56), @(103.9, 0), @(89.9, 0))
)

function Get-WinarchyLockScreenScale {
    param([Parameter(Mandatory)][int]$Width, [Parameter(Mandatory)][int]$Height)
    [Math]::Min($Width * 0.94 / 104, $Height * 0.9 / 56)
}

function Get-WinarchyPrimaryScreenSize {
    Add-Type -AssemblyName System.Windows.Forms
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    @{ Width = [Math]::Max(1920, $b.Width); Height = [Math]::Max(1080, $b.Height) }
}

function New-WinarchyAccentLockScreen {
    param(
        [Parameter(Mandatory)][string]$Accent,
        [Parameter(Mandatory)][string]$Background,
        [Parameter(Mandatory)][string]$Foreground,
        [Parameter(Mandatory)][string]$OutPath,
        [int]$Width = 1920,
        [int]$Height = 1080
    )
    Add-Type -AssemblyName System.Drawing
    $acc = ConvertTo-WinarchyDrawingColor $Accent
    $fg = ConvertTo-WinarchyDrawingColor $Foreground
    $bmp = [System.Drawing.Bitmap]::new($Width, $Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.SmoothingMode = 'AntiAlias'
        $g.Clear((ConvertTo-WinarchyDrawingColor $Background))

        $s = Get-WinarchyLockScreenScale -Width $Width -Height $Height
        $ox = ($Width - 104 * $s) / 2
        $oy = ($Height - 56 * $s) / 2
        $bar = {
            param($i)
            [System.Drawing.PointF[]]@($script:MarkBars[$i] | ForEach-Object {
                    [System.Drawing.PointF]::new([single]($ox + $_[0] * $s), [single]($oy + $_[1] * $s))
                })
        }

        $cx = $ox + 91 * $s; $cy = $oy + 30 * $s; $r = $Width * 0.5
        $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
        $path.AddEllipse($cx - $r, $cy - $r, 2 * $r, 2 * $r)
        $glow = [System.Drawing.Drawing2D.PathGradientBrush]::new($path)
        $glow.CenterPoint = [System.Drawing.PointF]::new($cx, $cy)
        $glow.CenterColor = [System.Drawing.Color]::FromArgb(62, $acc)
        $glow.SurroundColors = @([System.Drawing.Color]::FromArgb(0, $acc))
        $g.FillPath($glow, $path)

        $fill = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(11, $fg))
        $edge = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(26, $fg), 1.5)
        foreach ($i in 0..2) {
            $pts = [System.Drawing.PointF[]](& $bar $i)
            $g.FillPolygon($fill, $pts)
            $g.DrawPolygon($edge, $pts)
        }
        $pts = [System.Drawing.PointF[]](& $bar 3)
        $gradient = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
            [System.Drawing.PointF]::new(0, [single]$oy), [System.Drawing.PointF]::new(0, [single]($oy + 56 * $s)),
            [System.Drawing.Color]::FromArgb(52, $acc), [System.Drawing.Color]::FromArgb(120, $acc))
        $g.FillPolygon($gradient, $pts)
        $g.DrawPolygon([System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(235, $acc), 3), $pts)

        $noise = [System.Drawing.Bitmap]::new(128, 128)
        $rnd = [System.Random]::new(7)
        foreach ($x in 0..127) { foreach ($y in 0..127) { $noise.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($rnd.Next(0, 7), 255, 255, 255)) } }
        $g.FillRectangle([System.Drawing.TextureBrush]::new($noise), 0, 0, $Width, $Height)

        $bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $g.Dispose()
        $bmp.Dispose()
    }
}

function Update-WinarchyLockScreen {
    param([Parameter(Mandatory)][string]$ThemeDir, [Parameter(Mandatory)][System.Collections.IDictionary]$Colors, [switch]$Dynamic)
    $image = Get-WinarchyLockScreenImage -ThemeDir $ThemeDir -Dynamic:$Dynamic
    if (-not $image -and $Dynamic) {
        try {
            $image = Join-Path (Get-WinarchyStateDir) 'lockscreen-accent.png'
            $size = Get-WinarchyPrimaryScreenSize
            New-WinarchyAccentLockScreen -OutPath $image -Width $size.Width -Height $size.Height `
                -Accent (Get-WinarchyReadableAccentHex -Accent $Colors['accent'] -Background $Colors['background']) `
                -Background $Colors['background'] -Foreground $Colors['foreground']
        }
        catch {
            Write-WinarchyWarn "Lock screen image not generated: $($_.Exception.Message)"
            return
        }
    }
    if ($image) { Set-WinarchyLockScreen -ImagePath $image }
}
