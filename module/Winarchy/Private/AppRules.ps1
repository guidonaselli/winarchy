# AppRules.ps1 — reglas de komorebi por aplicación, resueltas por capas.
#
# ASC (las reglas de la comunidad, vendor/asc/applications.json) se COMPILA dentro del
# komorebi.json generado en vez de apuntarle con `app_specific_configuration_path`. Así
# la precedencia la define Winarchy y no komorebi, la salida es un único archivo
# inspeccionable, y no queda una ruta de terceros viva en runtime. Es el mismo patrón
# que `windows.toml` (ver Add-WinarchyWindowRulesToKomorebiJson).
#
# Capas, de menor a mayor prioridad:
#   1. vendor/asc/applications.json   comunidad, pinneado en versions.lock.toml
#   2. templates/komorebi.json.tpl    reglas del producto (ya renderizadas en el JSON)
#   3. games.toml                     catálogo de juegos (ya renderizado en el JSON)
#   4. config/komorebi/rules.toml     del usuario, gitignored — nada lo pisa nunca
#
# Las capas 2 y 3 llegan acá ya resueltas dentro del JSON renderizado, así que el merge
# real es entre tres bloques: ASC (abajo), lo que trae el JSON (medio) y rules.toml (arriba).

# Categorías de "ubicación": decidir dónde va una ventana es excluyente, así que la capa
# más alta que opina sobre un identificador reemplaza lo que dijeron las de abajo.
$script:AscCache = $null
$script:AscCachePath = $null

$script:AppRulePlacement = [ordered]@{
    ignore   = 'ignore_rules'
    manage   = 'manage_rules'
    floating = 'floating_applications'
}
# Categorías de "atributo": describen cómo se comporta la ventana, no dónde va. Nunca
# entran en conflicto entre sí, así que se unen sumando todas las capas.
$script:AppRuleAttribute = [ordered]@{
    layered               = 'layered_applications'
    object_name_change    = 'object_name_change_applications'
    tray_and_multi_window = 'tray_and_multi_window_applications'
    slow_application      = 'slow_application_identifiers'
    transparency_ignore   = 'transparency_ignore_rules'
}

function Get-WinarchyAppRuleCategories {
    <# Mapa clave de ASC -> clave del komorebi.json, en un solo lugar. #>
    $all = [ordered]@{}
    foreach ($m in @($script:AppRulePlacement, $script:AppRuleAttribute)) {
        foreach ($k in $m.Keys) { $all[$k] = $m[$k] }
    }
    $all
}

function Get-WinarchyAscPath { Join-Path (Get-WinarchyRoot) 'vendor\asc\applications.json' }

function Get-WinarchyAscAppCount {
    $path = Get-WinarchyAscPath
    if (-not (Test-Path $path)) { return 0 }
    @((Get-Content $path -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable).Keys | Where-Object { -not $_.StartsWith('$') }).Count
}
function Get-WinarchyUserRulesPath { Join-Path (Get-WinarchyRoot) 'config\komorebi\rules.toml' }

function ConvertTo-WinarchyRuleTarget {
    <#
      Identidad de "a qué ventana apunta" una regla, ignorando matching_strategy: dos
      reglas sobre el mismo kind+id hablan de la misma ventana aunque una use Equals y
      la otra Legacy, y si no se comparan así el override entre capas no dispara.
      Una regla compuesta (array = AND) apunta a la combinación de sus miembros.
    #>
    param([Parameter(Mandatory)]$Rule)
    $parts = foreach ($r in @($Rule)) { "$($r.kind)=$($r.id)".ToLowerInvariant() }
    ($parts | Sort-Object) -join '&'
}

function ConvertTo-WinarchyRuleKey {
    <# Identidad exacta, para deduplicar dentro de una misma categoría. #>
    param([Parameter(Mandatory)]$Rule)
    $parts = foreach ($r in @($Rule)) { "$($r.kind)=$($r.id)/$($r.matching_strategy)".ToLowerInvariant() }
    ($parts | Sort-Object) -join '&'
}

function ConvertTo-WinarchyRuleObject {
    <# Normaliza una regla (hashtable o PSCustomObject, simple o compuesta) a PSCustomObject. #>
    param([Parameter(Mandatory)]$Rule)
    $one = {
        param($r)
        $o = [ordered]@{ kind = $r.kind; id = $r.id }
        $strategy = if ($r -is [hashtable]) { $r['matching_strategy'] } else { $r.matching_strategy }
        if ($strategy) { $o['matching_strategy'] = $strategy }
        [pscustomobject]$o
    }
    if ($Rule -is [System.Collections.IEnumerable] -and $Rule -isnot [string] -and $Rule -isnot [hashtable]) {
        @(foreach ($r in $Rule) { & $one $r })
    }
    else { & $one $Rule }
}

function Get-WinarchyDisabledAscApps {
    <# Nombres de apps de ASC que el usuario desactivó por completo. #>
    param($UserRules)
    $names = [System.Collections.Generic.List[string]]::new()
    if ($UserRules -and $UserRules.ContainsKey('disable')) {
        foreach ($entry in $UserRules['disable']) {
            if ($entry.ContainsKey('asc') -and $entry['asc']) { $names.Add([string]$entry['asc']) }
        }
    }
    $names
}

function Get-WinarchyAscLayer {
    <#
      Aplana vendor/asc/applications.json a una lista de reglas etiquetadas con la app
      que las aporta. Las apps listadas en [[disable]] se saltean enteras: desactivar por
      nombre (y no por identificador) sobrevive a que upstream le agregue reglas nuevas.
    #>
    param([string[]]$Disabled = @())
    $path = Get-WinarchyAscPath
    if (-not (Test-Path $path)) { return @() }
    # Parsear 63 KB de JSON por cada render se nota: `theme set` lo hace una vez y la
    # suite de tests 23. El archivo es inmutable dentro del proceso, asi que se cachea.
    if (-not $script:AscCache -or $script:AscCachePath -ne $path) {
        $script:AscCache = Get-Content $path -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
        $script:AscCachePath = $path
    }
    $asc = $script:AscCache
    $categories = Get-WinarchyAppRuleCategories
    $skip = @($Disabled | Where-Object { $_ } | ForEach-Object { $_.ToLowerInvariant() })
    $out = [System.Collections.Generic.List[object]]::new()
    foreach ($app in ($asc.Keys | Sort-Object)) {
        if ($app.StartsWith('$')) { continue }
        if ($app.ToLowerInvariant() -in $skip) { continue }
        foreach ($key in $categories.Keys) {
            if (-not $asc[$app].ContainsKey($key)) { continue }
            foreach ($rule in $asc[$app][$key]) {
                $out.Add(@{
                        Category = $categories[$key]
                        Rule     = ConvertTo-WinarchyRuleObject -Rule $rule
                        Source   = "asc:$app"
                    })
            }
        }
    }
    $out
}

function Get-WinarchyUserRuleLayer {
    <#
      Lee config/komorebi/rules.toml. Cada categoría es un [[array de tablas]] con una
      entrada por regla; `exe`/`class`/`title` son azúcar sobre kind+id.
        [[ignore]]
        exe = "GalaxyClient.exe"
    #>
    param($UserRules)
    if (-not $UserRules) { return @() }
    $categories = Get-WinarchyAppRuleCategories
    $out = [System.Collections.Generic.List[object]]::new()
    foreach ($key in $categories.Keys) {
        if (-not $UserRules.ContainsKey($key)) { continue }
        foreach ($entry in $UserRules[$key]) {
            if ($entry.ContainsKey('exe')) { $kind = 'Exe'; $id = $entry['exe'] }
            elseif ($entry.ContainsKey('class')) { $kind = 'Class'; $id = $entry['class'] }
            elseif ($entry.ContainsKey('title')) { $kind = 'Title'; $id = $entry['title'] }
            else { throw "rules.toml: every [[$key]] entry needs one of exe/class/title." }
            $strategy = if ($entry.ContainsKey('matching_strategy')) { $entry['matching_strategy'] } else { 'Equals' }
            $out.Add(@{
                    Category = $categories[$key]
                    Rule     = [pscustomobject]@{ kind = $kind; id = $id; matching_strategy = $strategy }
                    Source   = 'user:rules.toml'
                })
        }
    }
    $out
}

function Get-WinarchyJsonRuleLayer {
    <# Las reglas que el komorebi.json renderizado ya trae (template + games.toml). #>
    param([Parameter(Mandatory)]$Config)
    $out = [System.Collections.Generic.List[object]]::new()
    foreach ($category in (Get-WinarchyAppRuleCategories).Values) {
        if (-not $Config.PSObject.Properties[$category]) { continue }
        foreach ($rule in @($Config.$category)) {
            if ($null -eq $rule) { continue }
            $out.Add(@{ Category = $category; Rule = $rule; Source = 'winarchy' })
        }
    }
    $out
}

function Merge-WinarchyAppRules {
    <#
      Resuelve las capas (de menor a mayor prioridad) en un mapa categoría -> reglas.

      Atributos: unión de todas las capas (describen comportamiento, no compiten).
      Ubicación: para cada ventana apuntada, gana la capa MÁS ALTA que opine, y sus
      categorías reemplazan las de abajo. Eso es lo que hace que un [[ignore]] tuyo le
      gane a un `manage` de ASC de verdad: no alcanza con sumar el ignore, hay que sacar
      el manage, porque las listas de komorebi son aditivas.
    #>
    # $Layers sin tipo a propósito: un [object[]] desenrolla las capas vacías y colapsa
    # la lista de listas en una lista plana de reglas, que hace perder los niveles.
    param([Parameter(Mandatory)]$Layers)

    $placement = @{}   # target -> @{ Level; Entries }
    $merged = [ordered]@{}
    foreach ($category in (Get-WinarchyAppRuleCategories).Values) {
        $merged[$category] = [System.Collections.Generic.List[object]]::new()
    }
    $placementCategories = @($script:AppRulePlacement.Values)
    $seen = @{}

    for ($level = 0; $level -lt $Layers.Count; $level++) {
        foreach ($entry in $Layers[$level]) {
            if ($entry.Category -in $placementCategories) {
                $target = ConvertTo-WinarchyRuleTarget -Rule $entry.Rule
                if (-not $placement.ContainsKey($target) -or $placement[$target].Level -lt $level) {
                    $placement[$target] = @{ Level = $level; Entries = [System.Collections.Generic.List[object]]::new() }
                }
                if ($placement[$target].Level -eq $level) { $placement[$target].Entries.Add($entry) }
                continue
            }
            $key = "$($entry.Category)|" + (ConvertTo-WinarchyRuleKey -Rule $entry.Rule)
            if ($seen.ContainsKey($key)) { continue }
            $seen[$key] = $true
            $merged[$entry.Category].Add($entry.Rule)
        }
    }

    foreach ($target in ($placement.Keys | Sort-Object)) {
        foreach ($entry in $placement[$target].Entries) {
            $key = "$($entry.Category)|" + (ConvertTo-WinarchyRuleKey -Rule $entry.Rule)
            if ($seen.ContainsKey($key)) { continue }
            $seen[$key] = $true
            $merged[$entry.Category].Add($entry.Rule)
        }
    }
    $merged
}

function Get-WinarchyAppRuleLayers {
    <# Las tres capas en orden de prioridad creciente, listas para Merge-WinarchyAppRules. #>
    param([Parameter(Mandatory)]$Config)
    $userRules = $null
    $userPath = Get-WinarchyUserRulesPath
    if (Test-Path $userPath) { $userRules = Import-WinarchyToml -Path $userPath }
    $layers = [System.Collections.Generic.List[object]]::new()
    $layers.Add([object[]]@(Get-WinarchyAscLayer -Disabled (Get-WinarchyDisabledAscApps -UserRules $userRules)))
    $layers.Add([object[]]@(Get-WinarchyJsonRuleLayer -Config $Config))
    $layers.Add([object[]]@(Get-WinarchyUserRuleLayer -UserRules $userRules))
    # La coma evita que el return desenrolle la lista de capas en sus reglas.
    , $layers
}

function Add-WinarchyAppRulesToKomorebiJson {
    <# Compila ASC + rules.toml dentro del komorebi.json renderizado. #>
    param([Parameter(Mandatory)][string]$Json)
    $config = $Json | ConvertFrom-Json
    $merged = Merge-WinarchyAppRules -Layers (Get-WinarchyAppRuleLayers -Config $config)
    foreach ($category in $merged.Keys) {
        $rules = @($merged[$category])
        if ($rules.Count -eq 0) {
            if ($config.PSObject.Properties[$category]) { $config.PSObject.Properties.Remove($category) }
            continue
        }
        if ($config.PSObject.Properties[$category]) { $config.$category = $rules }
        else { $config | Add-Member -NotePropertyName $category -NotePropertyValue $rules }
    }
    $config | ConvertTo-Json -Depth 50
}

function Get-WinarchyStaleIgnores {
    <# Ignores que komorebi tiene en memoria y el config ya no: replace-configuration suma reglas pero nunca las saca. #>
    param($Live, $Config)
    $keep = @{}
    foreach ($rule in @($Config)) { if ($null -ne $rule) { $keep[(ConvertTo-WinarchyRuleTarget -Rule $rule)] = $true } }
    foreach ($rule in @($Live)) {
        if ($null -eq $rule) { continue }
        $target = ConvertTo-WinarchyRuleTarget -Rule $rule
        if (-not $keep.ContainsKey($target)) { $target }
    }
}

function Get-WinarchyKomorebiHwnds {
    param([Parameter(Mandatory)]$State, [switch]$Floating)
    foreach ($m in $State.monitors.elements) {
        foreach ($w in $m.workspaces.elements) {
            @($w.floating_windows.elements) | Where-Object { $_ } | ForEach-Object { [long]$_.hwnd }
            if ($Floating) { continue }
            @($w.containers.elements) | ForEach-Object { @($_.windows.elements) } | Where-Object { $_ } | ForEach-Object { [long]$_.hwnd }
            if ($w.monocle_container) { @($w.monocle_container.windows.elements) | Where-Object { $_ } | ForEach-Object { [long]$_.hwnd } }
        }
    }
}

function Invoke-WinarchyKomorebiReplace {
    <# replace-configuration reconstruye el WM entero y, en komorebi 0.1.41, nunca vuelve a mostrar las flotantes de
       un workspace oculto: quedan ocultas y fuera del estado nuevo. Mostrarlas hace que komorebi las retome con sus reglas. #>
    param([Parameter(Mandatory)][string]$ConfigPath)
    Add-Type -Namespace Winarchy.Native -Name Cloak -ErrorAction SilentlyContinue -MemberDefinition @'
[DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int s);
[DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
public static bool IsCloaked(long h) { int v; return DwmGetWindowAttribute(new IntPtr(h), 14, out v, 4) == 0 && v != 0; }
[ComImport, Guid("6D5140C1-7436-11CE-8034-00AA006009FA"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IServiceProvider { [PreserveSig] int QueryService(ref Guid s, ref Guid r, out IntPtr o); }
[UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetViewForHwnd(IntPtr self, IntPtr hwnd, out IntPtr view);
[UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int SetCloak(IntPtr self, int type, int flags);
static T Slot<T>(IntPtr obj, int i) { return (T)(object)Marshal.GetDelegateForFunctionPointer(Marshal.ReadIntPtr(Marshal.ReadIntPtr(obj), i * IntPtr.Size), typeof(T)); }
// El mismo cloak de shell que usa komorebi (IApplicationViewCollection -> IApplicationView::SetCloak).
public static bool Show(long hwnd) {
    var shell = (IServiceProvider)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("C2F03A33-21F5-47FA-B4BB-156362A2F239")));
    var iid = new Guid("1841c6d7-4f9d-42c0-af41-8747538f10e5");
    IntPtr views, view;
    if (shell.QueryService(ref iid, ref iid, out views) != 0) return false;
    int hr = Slot<GetViewForHwnd>(views, 6)(views, new IntPtr(hwnd), out view);
    Marshal.Release(views);
    if (hr != 0 || view == IntPtr.Zero) return false;
    hr = Slot<SetCloak>(view, 12)(view, 1, 0);
    Marshal.Release(view);
    return hr == 0;
}
'@
    $state = komorebic state 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
    $hidden = @(if ($state) { Get-WinarchyKomorebiHwnds -State $state -Floating | Where-Object { [Winarchy.Native.Cloak]::IsCloaked($_) } })
    komorebic replace-configuration $ConfigPath 2>$null | Out-Null
    # El rebuild corre bajo el lock de komorebi: cuando las ocultas dejan de figurar, terminó.
    for ($i = 0; $hidden -and $i -lt 15; $i++) {
        $state = komorebic state 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
        if (-not $state) { Start-Sleep -Milliseconds 200; continue }
        $managed = @(Get-WinarchyKomorebiHwnds -State $state)
        if (-not @($hidden | Where-Object { $managed -contains $_ })) {
            $hidden | Where-Object { [Winarchy.Native.Cloak]::IsWindow([IntPtr]$_) -and [Winarchy.Native.Cloak]::IsCloaked($_) } |
                ForEach-Object { $null = [Winarchy.Native.Cloak]::Show($_) }
            break
        }
        Start-Sleep -Milliseconds 200
    }
    Restore-WinarchyFloatingApps -ConfigPath $ConfigPath
    Write-WinarchyStaleIgnoreWarning -ConfigPath $ConfigPath
}

function Get-WinarchyTiledFloatingExes {
    <# Exes con regla `floating_applications` que komorebi tiene dentro de un container: la regla solo rige para ventanas
       nuevas, así que tras un reinicio o un replace las que ya estaban abiertas quedan tileadas. #>
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)]$Config)
    $floating = @($Config.floating_applications | Where-Object { $_ -and $_.kind -eq 'Exe' -and $_.matching_strategy -eq 'Equals' } | ForEach-Object { $_.id.ToLowerInvariant() })
    if (-not $floating) { return }
    $tiled = foreach ($m in $State.monitors.elements) {
        foreach ($w in $m.workspaces.elements) {
            @($w.containers.elements) | ForEach-Object { @($_.windows.elements) } | Where-Object { $_ } | ForEach-Object { $_.exe }
        }
    }
    @($tiled | Where-Object { $_ -and $floating -contains $_.ToLowerInvariant() } | Select-Object -Unique)
}

function Restore-WinarchyFloatingApps {
    <# Pasa a flotante las ventanas ya abiertas que una regla `floating` debería tener así. Enfoca cada una un instante
       (komorebi solo alterna la enfocada) y devuelve el foco a la ventana de antes. #>
    param([Parameter(Mandatory)][string]$ConfigPath)
    Add-Type -Namespace Winarchy.Native -Name Fg -ErrorAction SilentlyContinue -MemberDefinition @'
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
'@
    if (-not (Test-Path -LiteralPath $ConfigPath)) { return }
    $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json -ErrorAction SilentlyContinue
    $state = komorebic state 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
    if (-not $config -or -not $state) { return }
    $exes = @(Get-WinarchyTiledFloatingExes -State $state -Config $config)
    if (-not $exes) { return }
    $fg = [Winarchy.Native.Fg]::GetForegroundWindow()
    foreach ($exe in $exes) {
        for ($n = 0; $n -lt 8; $n++) {
            komorebic eager-focus $exe 2>$null | Out-Null
            Start-Sleep -Milliseconds 250
            komorebic toggle-float 2>$null | Out-Null
            Start-Sleep -Milliseconds 250
            $state = komorebic state 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
            if (-not $state -or $exe -notin @(Get-WinarchyTiledFloatingExes -State $state -Config $config)) { break }
        }
    }
    $null = [Winarchy.Native.Fg]::SetForegroundWindow($fg)
}

function Write-WinarchyStaleIgnoreWarning {
    param([Parameter(Mandatory)][string]$ConfigPath)
    $live = komorebic global-state 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
    if (-not $live) { return }
    $stale = @(Get-WinarchyStaleIgnores -Live $live.ignore_identifiers -Config (Get-Content -Raw $ConfigPath | ConvertFrom-Json).ignore_rules)
    if ($stale.Count -eq 0) { return }
    Write-WinarchyWarn "komorebi still ignores $($stale -join ', ') until it restarts: komorebic stop; winarchy reload (open windows of those apps may stay hidden)."
}

function Get-WinarchyAppRuleReport {
    <#
      Qué reglas aplican a un identificador y qué capa ganó. Sin esto, 226 entradas que
      no escribiste son una caja negra y debuguear una ventana rara es adivinar.
    #>
    param([Parameter(Mandatory)][string]$Match)
    $jsonPath = Join-Path (Get-WinarchyRoot) 'config\komorebi\komorebi.json'
    if (-not (Test-Path $jsonPath)) { throw 'komorebi.json not generated yet. Run: winarchy theme set <name>' }
    $config = Get-Content $jsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $layers = Get-WinarchyAppRuleLayers -Config $config
    $merged = Merge-WinarchyAppRules -Layers $layers

    $needle = $Match.ToLowerInvariant()
    $winning = @{}
    foreach ($category in $merged.Keys) {
        foreach ($rule in $merged[$category]) {
            $winning["$category|" + (ConvertTo-WinarchyRuleKey -Rule $rule)] = $true
        }
    }
    $layerNames = @('asc', 'winarchy', 'user')
    $out = [System.Collections.Generic.List[object]]::new()
    for ($level = 0; $level -lt $layers.Count; $level++) {
        foreach ($entry in $layers[$level]) {
            $ids = @(foreach ($r in @($entry.Rule)) { [string]$r.id })
            if (-not ($ids | Where-Object { $_ -and $_.ToLowerInvariant().Contains($needle) })) { continue }
            $key = "$($entry.Category)|" + (ConvertTo-WinarchyRuleKey -Rule $entry.Rule)
            $out.Add([pscustomobject]@{
                    Layer    = $layerNames[$level]
                    Source   = $entry.Source
                    Category = $entry.Category
                    Rule     = ($ids -join ' + ')
                    Applied  = [bool]$winning[$key]
                })
        }
    }
    $out
}

function Get-WinarchyAppRuleCollisions {
    <#
      Reglas de ubicacion de ASC que perdieron contra una capa superior QUE DECIDIO OTRA
      COSA. Que ASC y Winarchy digan los dos `ignore` sobre la misma ventana no es una
      colision, es un duplicado: reportarlo solo enterraria las colisiones de verdad.
    #>
    param($Config)
    if (-not $Config) {
        $jsonPath = Join-Path (Get-WinarchyRoot) 'config\komorebi\komorebi.json'
        if (-not (Test-Path $jsonPath)) { return @() }
        $Config = Get-Content $jsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    $layers = Get-WinarchyAppRuleLayers -Config $Config
    $merged = Merge-WinarchyAppRules -Layers $layers
    $placementCategories = @($script:AppRulePlacement.Values)

    $winner = @{}
    foreach ($category in $placementCategories) {
        foreach ($rule in $merged[$category]) {
            $target = ConvertTo-WinarchyRuleTarget -Rule $rule
            if (-not $winner.ContainsKey($target)) { $winner[$target] = [System.Collections.Generic.List[string]]::new() }
            if ($category -notin $winner[$target]) { $winner[$target].Add($category) }
        }
    }

    $out = [System.Collections.Generic.List[object]]::new()
    $seen = @{}
    foreach ($entry in $layers[0]) {
        if ($entry.Category -notin $placementCategories) { continue }
        $target = ConvertTo-WinarchyRuleTarget -Rule $entry.Rule
        if ($winner.ContainsKey($target) -and $entry.Category -in $winner[$target]) { continue }
        $key = "$target|$($entry.Category)"
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        $out.Add([pscustomobject]@{
                App     = ($entry.Source -replace '^asc:', '')
                Rule    = (@(foreach ($r in @($entry.Rule)) { [string]$r.id }) -join ' + ')
                AscSaid = $entry.Category
                Winner  = $(if ($winner.ContainsKey($target)) { $winner[$target] -join ', ' } else { 'dropped' })
            })
    }
    $out
}

function Add-WinarchyUserRule {
    <# Appendea una entrada a config/komorebi/rules.toml. #>
    param(
        [Parameter(Mandatory)][string]$Category,
        [Parameter(Mandatory)][ValidateSet('exe', 'class', 'title', 'asc')][string]$Field,
        [Parameter(Mandatory)][string]$Value,
        [string]$MatchingStrategy
    )
    $validCategories = @((Get-WinarchyAppRuleCategories).Keys) + 'disable'
    if ($Category -notin $validCategories) {
        throw "Unknown rule category '$Category'. Valid: $($validCategories -join ', ')"
    }
    if ($Category -eq 'disable' -and $Field -ne 'asc') {
        throw "winarchy rules add disable needs the 'asc' field: winarchy rules add disable asc `"<App Name>`""
    }
    if ($Category -ne 'disable' -and $Field -eq 'asc') {
        throw "'asc' is only valid with the 'disable' category."
    }

    $path = Get-WinarchyUserRulesPath
    if (-not (Test-Path $path)) {
        $example = "$path.example"
        if (Test-Path $example) { Copy-Item $example $path } else { New-Item -ItemType File -Path $path | Out-Null }
    }

    $existing = Import-WinarchyToml -Path $path
    if ($existing -and $existing.ContainsKey($Category)) {
        foreach ($entry in $existing[$Category]) {
            if ($entry.ContainsKey($Field) -and [string]$entry[$Field] -eq $Value) {
                Write-WinarchyInfo "Already in rules.toml: [[$Category]] $Field = `"$Value`""
                return
            }
        }
    }

    $lines = @('', "[[$Category]]", "$Field = `"$Value`"")
    if ($MatchingStrategy) { $lines += "matching_strategy = `"$MatchingStrategy`"" }
    Add-Content -Path $path -Value ($lines -join "`n") -Encoding UTF8
    Write-WinarchyOk "Added [[$Category]] $Field = `"$Value`" to $path"
    Update-WinarchyKomorebiRules
    if (Test-WinarchyProcess 'komorebi') { komorebic retile 2>$null | Out-Null }
}

function Sync-WinarchyAsc {
    <#
      Refresca vendor/asc/applications.json desde upstream y muestra el diff por app.
      NO toca config/komorebi/rules.toml: es otra capa, no el mismo archivo. Con -Apply
      escribe el archivo y actualiza versions.lock.toml; sin él solo reporta.
    #>
    param([switch]$Apply)
    $root = Get-WinarchyRoot
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $repo = $lock['asc']['repo']
    $url = "https://raw.githubusercontent.com/$repo/master/applications.json"

    Write-WinarchyInfo "Fetching $url ..."
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) 'winarchy-asc.json'
    try { Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop }
    catch { Write-WinarchyErr "Could not fetch ASC: $($_.Exception.Message)"; return }

    $newHash = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLower()
    if ($newHash -eq $lock['asc']['sha256']) { Write-WinarchyOk 'ASC already at the pinned revision.'; return }

    $current = Get-Content (Get-WinarchyAscPath) -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    $incoming = Get-Content $tmp -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    $names = @($current.Keys) + @($incoming.Keys) | Where-Object { -not $_.StartsWith('$') } | Sort-Object -Unique

    $userPath = Get-WinarchyUserRulesPath
    $userRules = if (Test-Path $userPath) { Import-WinarchyToml -Path $userPath } else { $null }
    $disabled = @(Get-WinarchyDisabledAscApps -UserRules $userRules | ForEach-Object { $_.ToLowerInvariant() })

    $added = @(); $removed = @(); $changed = @()
    foreach ($n in $names) {
        $a = if ($current.ContainsKey($n)) { $current[$n] | ConvertTo-Json -Depth 20 -Compress } else { $null }
        $b = if ($incoming.ContainsKey($n)) { $incoming[$n] | ConvertTo-Json -Depth 20 -Compress } else { $null }
        if ($a -eq $b) { continue }
        if (-not $a) { $added += $n } elseif (-not $b) { $removed += $n } else { $changed += $n }
    }

    Write-Host ''
    Write-Host "  ASC diff  (+$($added.Count) added, ~$($changed.Count) changed, -$($removed.Count) removed)"
    foreach ($set in @(@{ Mark = '+'; Items = $added }, @{ Mark = '~'; Items = $changed }, @{ Mark = '-'; Items = $removed })) {
        foreach ($n in $set.Items) {
            $flag = if ($n.ToLowerInvariant() -in $disabled) { '   [disabled in your rules.toml]' } else { '' }
            Write-Host "    $($set.Mark) $n$flag"
        }
    }
    Write-Host ''

    if (-not $Apply) {
        Write-WinarchyInfo 'Dry run. Re-run with --apply to vendor this revision.'
        return
    }
    Copy-Item $tmp (Get-WinarchyAscPath) -Force
    $lockPath = Join-Path $root 'versions.lock.toml'
    $text = Get-Content $lockPath -Raw -Encoding UTF8
    $text = $text -replace '(?m)^(sha256 = ")[0-9a-f]{64}(")', "`${1}$newHash`${2}"
    $text = $text -replace '(?m)^(date = ")\d{4}-\d{2}-\d{2}(")', "`${1}$(Get-Date -Format 'yyyy-MM-dd')`${2}"
    Set-Content -Path $lockPath -Value $text -Encoding UTF8 -NoNewline
    Write-WinarchyOk 'ASC vendored. Run `winarchy theme set <name>` to recompile komorebi.json.'
}
