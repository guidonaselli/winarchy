# Gallery.ps1 — `winarchy theme gallery`: visual theme picker (WPF grid of preview cards)

function Show-WinarchyThemeGallery {
    <#
      Omarchy-style theme picker: grid of preview.png cards, click to apply.
      Cards without a preview get rendered on the fly. Esc closes.
    #>
    Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

    $themesDir = Get-WinarchyThemesDir
    $current = Get-WinarchyCurrentTheme
    $activeTheme = if ($current) {
        try { Import-WinarchyToml -Path (Join-Path $themesDir "$current\theme.toml") } catch { $null }
    }
    $bg = if ($activeTheme) { $activeTheme['colors']['background'] } else { '#16121E' }
    $fg = if ($activeTheme) { $activeTheme['colors']['foreground'] } else { '#C8C0D8' }
    $accent = if ($activeTheme) { $activeTheme['colors']['accent'] } else { '#9BE53E' }

    $window = [System.Windows.Window]::new()
    # Title doubles as komorebi ignore-rule match (floats instead of tiling)
    $window.Title = 'Winarchy — Themes'
    $window.Width = 1180; $window.Height = 760
    $window.WindowStartupLocation = 'CenterScreen'
    $window.Background = [System.Windows.Media.BrushConverter]::new().ConvertFromString($bg)
    # Borderless picker: Esc closes, drag anywhere to move
    $window.WindowStyle = 'None'
    $window.ResizeMode = 'NoResize'
    $window.Add_MouseLeftButtonDown({ param($sender, $e) try { $window.DragMove() } catch {} }.GetNewClosure())

    $scroll = [System.Windows.Controls.ScrollViewer]::new()
    $scroll.VerticalScrollBarVisibility = 'Auto'
    $wrap = [System.Windows.Controls.WrapPanel]::new()
    $wrap.Margin = [System.Windows.Thickness]::new(12)
    $scroll.Content = $wrap

    $accentBrush = [System.Windows.Media.BrushConverter]::new().ConvertFromString($accent)
    # Accent frame so the borderless window reads as a deliberate picker
    $frame = [System.Windows.Controls.Border]::new()
    $frame.BorderBrush = $accentBrush
    $frame.BorderThickness = [System.Windows.Thickness]::new(2)
    $frame.Child = $scroll
    $window.Content = $frame

    $fgBrush = [System.Windows.Media.BrushConverter]::new().ConvertFromString($fg)
    $dimBrush = [System.Windows.Media.SolidColorBrush]::new([System.Windows.Media.Color]::FromArgb(60, 128, 128, 128))

    foreach ($name in Get-WinarchyThemes) {
        $previewPath = Join-Path $themesDir "$name\preview.png"
        if (-not (Test-Path $previewPath)) {
            try { $previewPath = New-WinarchyThemePreview -Name $name } catch { continue }
        }

        $border = [System.Windows.Controls.Border]::new()
        $border.Margin = [System.Windows.Thickness]::new(8)
        $border.BorderThickness = [System.Windows.Thickness]::new(2)
        $border.CornerRadius = [System.Windows.CornerRadius]::new(6)
        $border.BorderBrush = if ($name -eq $current) { $accentBrush } else { $dimBrush }
        $border.Cursor = [System.Windows.Input.Cursors]::Hand
        $border.Tag = $name

        $stack = [System.Windows.Controls.StackPanel]::new()

        $img = [System.Windows.Controls.Image]::new()
        $bmp = [System.Windows.Media.Imaging.BitmapImage]::new()
        $bmp.BeginInit()
        $bmp.UriSource = [Uri]::new($previewPath)
        $bmp.CacheOption = 'OnLoad'   # do not lock the file: previews get regenerated
        $bmp.DecodePixelWidth = 680
        $bmp.EndInit()
        $img.Source = $bmp
        $img.Width = 340
        $img.Margin = [System.Windows.Thickness]::new(4, 4, 4, 0)

        $label = [System.Windows.Controls.TextBlock]::new()
        $label.Text = if ($name -eq $current) { "$name  (active)" } else { $name }
        $label.Foreground = $fgBrush
        $label.FontSize = 13
        $label.HorizontalAlignment = 'Center'
        $label.Margin = [System.Windows.Thickness]::new(0, 4, 0, 6)

        $stack.AddChild($img)
        $stack.AddChild($label)
        $border.Child = $stack

        $border.Add_MouseLeftButtonUp({
            param($sender, $e)
            $window.Close()
            Set-WinarchyTheme -Name $sender.Tag
        }.GetNewClosure())

        $wrap.AddChild($border)
    }

    $window.Add_KeyDown({
        param($sender, $e)
        if ($e.Key -eq 'Escape') { $window.Close() }
    }.GetNewClosure())

    [void]$window.ShowDialog()
}
