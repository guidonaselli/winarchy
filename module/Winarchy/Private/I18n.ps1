function Get-WinarchyUiLanguage {
    param([string]$Language)

    if (-not $Language) {
        $Language = if ($env:WINARCHY_LANG) { $env:WINARCHY_LANG } else { (Get-Culture).Name }
    }
    if ($Language -match '^es(?:[-_]|$)') { 'es' } else { 'en' }
}

function Get-WinarchyUiText {
    param(
        [Parameter(Mandatory)][ValidateSet(
            'theme-gallery-title', 'theme-gallery-active',
            'background-gallery-title', 'background-gallery-empty',
            'background-gallery-dynamic', 'background-gallery-current',
            'background-gallery-no-theme', 'background-gallery-apply-error'
        )][string]$Key,
        [string]$Language
    )

    $messages = @{
        'theme-gallery-title' = @('Themes', 'Temas')
        'theme-gallery-active' = @('Active', 'Activo')
        'background-gallery-title' = @('Wallpapers — {0}', 'Fondos — {0}')
        'background-gallery-empty' = @('This theme has no bundled wallpapers.', 'Este theme no incluye fondos de pantalla.')
        'background-gallery-dynamic' = @('Auto Accent follows Windows colors. Choose a wallpaper in Helium or Wallpaper Engine.', 'Auto Accent sigue los colores de Windows. Elegí el fondo en Helium o Wallpaper Engine.')
        'background-gallery-current' = @('Current wallpaper', 'Fondo actual')
        'background-gallery-no-theme' = @('No theme applied yet; run winarchy theme set <name> first.', 'Todavía no hay un theme aplicado; primero ejecutá winarchy theme set <nombre>.')
        'background-gallery-apply-error' = @('Wallpaper ''{0}'' was not applied: {1}', 'No se pudo aplicar el fondo ''{0}'': {1}')
    }
    $index = if ((Get-WinarchyUiLanguage -Language $Language) -eq 'es') { 1 } else { 0 }
    $messages[$Key][$index]
}
