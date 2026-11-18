function Get-WinarchyBackgroundGalleryText {
    param(
        [Parameter(Mandatory)][ValidateSet('title', 'empty', 'dynamic', 'current', 'no-theme', 'apply-error')][string]$Key,
        [string]$Theme,
        [string]$File,
        [string]$Reason,
        [string]$Language
    )

    $text = Get-WinarchyUiText -Key "background-gallery-$Key" -Language $Language
    if ($Key -eq 'title') { return $text -f $Theme }
    if ($Key -eq 'apply-error') { return $text -f $File, $Reason }
    $text
}

function Get-WinarchyBackgroundGalleryLayout {
    param(
        [Parameter(Mandatory)][double]$WorkWidth,
        [Parameter(Mandatory)][double]$WorkHeight,
        [Parameter(Mandatory)][int]$Count
    )

    $width = [Math]::Min(1120, [Math]::Max(320, $WorkWidth - 48))
    $cardWidth = [Math]::Min(346, [Math]::Max(220, [Math]::Floor(($width - 112) / 3)))
    $imageHeight = [Math]::Round(($cardWidth - 24) * 9 / 16)
    $cardHeight = $imageHeight + 62
    $columns = [Math]::Max(1, [Math]::Floor(($width - 44) / ($cardWidth + 12)))
    $rows = [Math]::Max(1, [Math]::Ceiling($Count / $columns))
    $maxHeight = [Math]::Max(320, $WorkHeight - 48)

    [pscustomobject]@{
        Width = $width
        Height = [Math]::Min($maxHeight, $(if ($Count -eq 0) { 280 } else { 116 + $rows * ($cardHeight + 12) }))
        CardWidth = $cardWidth
        Columns = $columns
        Rows = $rows
    }
}

function Show-WinarchyBackgroundGallery {
    Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

    $themeName = Get-WinarchyCurrentTheme
    if (-not $themeName) { throw (Get-WinarchyBackgroundGalleryText -Key no-theme) }

    $theme = Import-WinarchyToml -Path (Join-Path (Join-Path (Get-WinarchyThemesDir) $themeName) 'theme.toml')
    $dynamic = [bool]($theme.ContainsKey('dynamic_accent') -and $theme['dynamic_accent'])
    if ($dynamic) { $null = Set-WinarchyThemeLiveAccent -Theme $theme }

    $colors = $theme['colors']
    $background = [System.Windows.Media.BrushConverter]::new().ConvertFromString($colors['background'])
    $foreground = [System.Windows.Media.BrushConverter]::new().ConvertFromString($colors['foreground'])
    $accent = [System.Windows.Media.BrushConverter]::new().ConvertFromString($colors['accent'])
    $cardBackground = $background
    $cardForeground = $foreground
    $muted = [System.Windows.Media.BrushConverter]::new().ConvertFromString($colors['foreground'])
    $muted.Opacity = 0.38

    $window = [System.Windows.Window]::new()
    $window.Title = "Winarchy — $(Get-WinarchyBackgroundGalleryText -Key title -Theme $themeName)"
    $iconPath = Join-Path (Get-WinarchyRoot) 'assets\logo\winarchy.ico'
    if (Test-Path $iconPath) { $window.Icon = [System.Windows.Media.Imaging.BitmapFrame]::Create([Uri]::new($iconPath)) }
    $workArea = [System.Windows.SystemParameters]::WorkArea
    $files = @(Get-WinarchyBackgrounds -Name $themeName)
    $layout = Get-WinarchyBackgroundGalleryLayout -WorkWidth $workArea.Width -WorkHeight $workArea.Height -Count $files.Count
    $window.Width = $layout.Width
    $window.Height = $layout.Height
    $cardWidth = $layout.CardWidth
    $imageWidth = $cardWidth - 24
    $imageHeight = [Math]::Round($imageWidth * 9 / 16)
    $cardHeight = $imageHeight + 62
    $window.MinWidth = [Math]::Min(720, $window.Width)
    $window.MinHeight = [Math]::Min(280, $window.Height)
    $window.WindowStartupLocation = 'CenterScreen'
    $window.Background = $background

    $frame = [System.Windows.Controls.Border]::new()
    $frame.BorderBrush = $accent
    $frame.BorderThickness = [System.Windows.Thickness]::new(2)
    $frame.Padding = [System.Windows.Thickness]::new(18)

    $root = [System.Windows.Controls.DockPanel]::new()
    $status = [System.Windows.Controls.TextBlock]::new()
    $status.Foreground = $accent
    $status.FontSize = 14
    $status.Margin = [System.Windows.Thickness]::new(4, 0, 4, 12)
    [System.Windows.Controls.DockPanel]::SetDock($status, 'Top')
    $root.Children.Add($status) | Out-Null

    $scroll = [System.Windows.Controls.ScrollViewer]::new()
    $scroll.VerticalScrollBarVisibility = 'Auto'
    $scroll.HorizontalScrollBarVisibility = 'Disabled'
    $wrap = [System.Windows.Controls.WrapPanel]::new()
    $wrap.Margin = [System.Windows.Thickness]::new(2)
    $wrap.ItemWidth = $cardWidth + 12
    $wrap.ItemHeight = $cardHeight + 12
    $scroll.Content = $wrap
    $root.Children.Add($scroll) | Out-Null
    $frame.Child = $root
    $window.Content = $frame

    if ($files.Count -eq 0) {
        $status.Text = if ($dynamic) {
            Get-WinarchyBackgroundGalleryText -Key dynamic
        }
        else {
            Get-WinarchyBackgroundGalleryText -Key empty
        }
        $message = [System.Windows.Controls.TextBlock]::new()
        $message.Text = $status.Text
        $message.Foreground = $foreground
        $message.FontSize = 20
        $message.TextWrapping = 'Wrap'
        $message.HorizontalAlignment = 'Center'
        $message.VerticalAlignment = 'Center'
        $message.Margin = [System.Windows.Thickness]::new(32)
        $root.Children.Remove($scroll) | Out-Null
        $root.Children.Add($message) | Out-Null
    }
    else {
        $selected = Get-WinarchyBackground -Name $themeName
        $currentLabel = Get-WinarchyBackgroundGalleryText -Key current
        $status.Text = "$($currentLabel): $selected"
        foreach ($file in $files) {
            $button = [System.Windows.Controls.Button]::new()
            $button.Width = $cardWidth
            $button.Height = $cardHeight
            $button.Margin = [System.Windows.Thickness]::new(6)
            $button.Padding = [System.Windows.Thickness]::new(6)
            $button.Background = $cardBackground
            $button.Foreground = $cardForeground
            $button.BorderBrush = if ($file -eq $selected) { $accent } else { $muted }
            $button.BorderThickness = [System.Windows.Thickness]::new($(if ($file -eq $selected) { 3 } else { 1 }))
            $button.Template = [System.Windows.Markup.XamlReader]::Parse(@"
<ControlTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" TargetType="{x:Type Button}">
  <Border x:Name="card" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="8" ClipToBounds="True">
    <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center" />
  </Border>
  <ControlTemplate.Triggers>
    <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="card" Property="Opacity" Value="0.88" /></Trigger>
    <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="card" Property="BorderBrush" Value="$($colors['accent'])" /><Setter TargetName="card" Property="BorderThickness" Value="4" /></Trigger>
  </ControlTemplate.Triggers>
</ControlTemplate>
"@)
            $button.Cursor = [System.Windows.Input.Cursors]::Hand
            $button.Tag = $file

            $stack = [System.Windows.Controls.StackPanel]::new()
            $image = [System.Windows.Controls.Image]::new()
            $bitmap = [System.Windows.Media.Imaging.BitmapImage]::new()
            $bitmap.BeginInit()
            $bitmap.UriSource = [Uri]::new((Join-Path (Join-Path (Join-Path (Get-WinarchyThemesDir) $themeName) 'backgrounds') $file))
            $bitmap.CacheOption = 'OnLoad'
            $bitmap.DecodePixelWidth = 620
            $bitmap.EndInit()
            $image.Source = $bitmap
            $image.Width = $imageWidth
            $image.Height = $imageHeight
            $image.Stretch = 'UniformToFill'
            $image.Margin = [System.Windows.Thickness]::new(0, 0, 0, 6)

            $label = [System.Windows.Controls.TextBlock]::new()
            $label.Text = if ($file -eq $selected) { "$file — $currentLabel" } else { $file }
            $label.TextTrimming = 'CharacterEllipsis'
            $label.Foreground = $cardForeground
            $label.FontSize = 13
            $label.Margin = [System.Windows.Thickness]::new(4, 2, 4, 0)

            $stack.Children.Add($image) | Out-Null
            $stack.Children.Add($label) | Out-Null
            $button.Content = $stack
            [System.Windows.Automation.AutomationProperties]::SetName($button, $(if ($file -eq $selected) { "$file, $currentLabel" } else { $file }))

            $button.Add_Click({
                param($sender, $e)
                $chosen = [string]$sender.Tag
                $window.Close()
                try { Set-WinarchyBackground -File $chosen }
                catch { Write-WinarchyErr (Get-WinarchyBackgroundGalleryText -Key apply-error -File $chosen -Reason $_.Exception.Message) }
            }.GetNewClosure())
            $wrap.Children.Add($button) | Out-Null
        }

        $window.Add_Loaded({
            foreach ($button in $wrap.Children) {
                if ($button.Tag -eq $selected) {
                    $button.Focus() | Out-Null
                    break
                }
            }
        }.GetNewClosure())
    }

    $window.Add_KeyDown({
        param($sender, $e)
        if ($e.Key -eq 'Escape') { $window.Close() }
    }.GetNewClosure())

    [void]$window.ShowDialog()
}
