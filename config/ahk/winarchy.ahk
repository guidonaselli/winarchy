; ============================================================================
; winarchy.ahk — único dueño de hotkeys globales de Winarchy (AHK v2)
; Esquema SUPER estilo Omarchy → despacha a komorebic / Flow / Terminal / winarchy
; Absorbe el viejo win-space-launcher.ahk (Win+Space → Flow Launcher).
; ============================================================================
#Requires AutoHotkey v2.0
#SingleInstance Force
ProcessSetPriority "High"   ; el dispatcher debe responder siempre, costo ~0

; --- Rutas (el script vive en <repo>\config\ahk) -----------------------------
RepoRoot := RegExReplace(A_ScriptDir, "\\config\\ahk$")
StateDir := RepoRoot "\state"
GameFlag := StateDir "\game-mode.flag"
GamesToml := RepoRoot "\games.toml"
WinarchyPs1 := RepoRoot "\bin\winarchy.ps1"
DirCreate(StateDir)

; PID propio para que `winarchy doctor` nos detecte sin depender de WMI/CIM
try FileOpen(StateDir "\ahk.pid", "w").Write(DllCall("GetCurrentProcessId"))

; --- Config de usuario ---------------------------------------------------------
; true  = Win sola abre el menú Inicio (comportamiento nativo de Windows)
; false = Win sola es solo modificador (no abre nada)
WinAloneOpensStart := true

#HotIf !WinAloneOpensStart
~LWin::Send('{Blind}{vkE8}')    ; traga el key-up de Win sola sin afectar combos
#HotIf

; --- Helpers -----------------------------------------------------------------
; Ruta completa: no depender del PATH heredado (puede ser anterior a la instalación)
KomorebicExe := FileExist(A_ProgramFiles "\komorebi\bin\komorebic.exe")
    ? A_ProgramFiles "\komorebi\bin\komorebic.exe"
    : "komorebic.exe"

Komorebic(cmd) {
    Run('"' KomorebicExe '" ' cmd, , 'Hide')
}

Winarchy(args) {
    Run('pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' WinarchyPs1 '" ' args, , 'Hide')
}

WinarchyTerminal(args) {
    Run('wt.exe pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' WinarchyPs1 '" ' args)
}

ToggleFlow() {
    ; Flow Launcher es single-instance: relanzarlo togglea la ventana de búsqueda.
    ; La instancia en background no siempre puede tomar el foreground (lock de
    ; Windows), así que AHK activa la ventana para que el foco quede en el query.
    flow := EnvGet('LOCALAPPDATA') '\FlowLauncher\Flow.Launcher.exe'
    if !FileExist(flow)
        return
    Run('"' flow '"')
    hwnd := WinWait('Flow.Launcher ahk_exe Flow.Launcher.exe', , 2)
    ; Cuando el toggle OCULTA la ventana, puede desaparecer entre el WinWait
    ; y el WinActivate: en ese caso no hay nada que activar.
    if hwnd
        try WinActivate('ahk_id ' hwnd)
}

DefaultBrowser() {
    try {
        progId := RegRead('HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\http\UserChoice', 'ProgId')
        cmd := RegRead('HKCR\' progId '\shell\open\command')
        exe := RegExReplace(cmd, '^"([^"]+)".*$', '$1')
        if FileExist(exe)
            return exe
    }
    return 'msedge.exe'
}

WebApp(url) {
    ; ventana propia sin chrome de browser (--app=, Chromium); Firefox abre pestaña
    Run('"' DefaultBrowser() '" --app=' url)
}

LaunchMusic() {
    spotify := EnvGet('APPDATA') '\Spotify\Spotify.exe'
    if FileExist(spotify)
        Run('"' spotify '"')
    else
        WebApp('https://music.youtube.com')
}

LaunchObsidian() {
    exe := EnvGet('LOCALAPPDATA') '\Obsidian\Obsidian.exe'
    if FileExist(exe)
        Run('"' exe '"')
    else
        TrayTip('Obsidian no está instalado', 'Winarchy')
}

; --- Overlay de keybindings (SUPER+K, estilo Omarchy) ----------------------------
; El contenido se parsea del propio script (+ user.ahk): hotkey + comentario inline.
; Colores del theme actual vía config\ahk\theme.ini (generado por el theme engine).
KeyOverlay := ''

ParseKeymap(path, defaultTitle := '') {
    sections := []
    cur := ''
    if (defaultTitle != '') {
        cur := {title: defaultTitle, items: []}
        sections.Push(cur)
    }
    for line in StrSplit(FileRead(path, 'UTF-8'), '`n') {
        line := RTrim(line, '`r')
        if RegExMatch(line, '^; --- (.+?) -{2,}', &m) {
            cur := {title: Trim(m[1]), items: []}
            sections.Push(cur)
            continue
        }
        if !IsObject(cur) || !RegExMatch(line, '^#([+^!]*)(.+?)::(.*)$', &m)
            continue
        mods := m[1], key := m[2], rest := m[3]
        ; rangos repetitivos colapsados a una entrada (workspaces 1..9, flechas)
        if RegExMatch(key, '^[2-9]$') || key = 'Right' || key = 'Up' || key = 'Down'
            continue
        if (key = '1')
            key := '1..9'
        else if (key = 'Left')
            key := Chr(0x2190) '/' Chr(0x2192) '/' Chr(0x2191) '/' Chr(0x2193)
        desc := RegExMatch(rest, ';\s*(.+)$', &md) ? Trim(md[1]) : Trim(rest)
        disp := 'SUPER+' (InStr(mods, '+') ? 'Shift+' : '') (InStr(mods, '^') ? 'Ctrl+' : '') (InStr(mods, '!') ? 'Alt+' : '') key
        cur.items.Push({keys: disp, desc: desc})
    }
    return sections
}

ToggleKeyOverlay() {
    global KeyOverlay
    if (KeyOverlay != '') {
        CloseKeyOverlay()
        return
    }
    ; colores del theme (fallback Tokyo Night si todavía no se generó theme.ini)
    bg := '1a1b26', fg := 'c0caf5', ac := '7aa2f7', mut := '565f89'
    ini := A_ScriptDir '\theme.ini'
    if FileExist(ini) {
        bg := LTrim(IniRead(ini, 'colors', 'background', bg), '#')
        fg := LTrim(IniRead(ini, 'colors', 'foreground', fg), '#')
        ac := LTrim(IniRead(ini, 'colors', 'accent', ac), '#')
        mut := LTrim(IniRead(ini, 'colors', 'muted', mut), '#')
    }
    sections := ParseKeymap(A_ScriptFullPath)
    if FileExist(A_ScriptDir '\user.ahk')
        for s in ParseKeymap(A_ScriptDir '\user.ahk', 'Usuario')
            sections.Push(s)
    visible := []
    total := 0
    for s in sections {
        if !s.items.Length
            continue
        visible.Push(s)
        total += s.items.Length + 2          ; header + fila de aire
    }
    if !visible.Length
        return

    g := Gui('-Caption +AlwaysOnTop +ToolWindow', 'Winarchy Keybindings')
    g.BackColor := bg
    g.MarginX := 0, g.MarginY := 0

    rowH := 24, keyW := 170, descW := 230, gap := 12, pad := 30
    colW := keyW + gap + descW + 28
    cols := total > 34 ? 3 : 2
    maxRows := Ceil(total / cols)
    col := 0, row := 0, usedRows := 0
    for s in visible {
        blk := s.items.Length + 2
        if (col < cols - 1 && row > 0 && row + blk > maxRows + 1)
            col += 1, row := 0
        x := pad + col * colW
        g.SetFont('s10 bold', 'JetBrainsMono Nerd Font')
        g.Add('Text', Format('x{} y{} w{} c{}', x, pad + row * rowH, keyW + gap + descW, ac), StrUpper(s.title))
        row += 1
        g.SetFont('s10 norm', 'JetBrainsMono Nerd Font')
        for it in s.items {
            y := pad + row * rowH
            g.Add('Text', Format('x{} y{} w{} c{}', x, y, keyW, fg), it.keys)
            g.Add('Text', Format('x{} y{} w{} c{}', x + keyW + gap, y, descW, mut), it.desc)
            row += 1
        }
        row += 1
        if (row > usedRows)
            usedRows := row
    }
    w := pad * 2 + cols * colW - 28
    h := pad * 2 + (usedRows - 1) * rowH

    ; centrado en el monitor donde está el mouse
    CoordMode('Mouse', 'Screen')
    MouseGetPos(&mx, &my)
    wl := 0, wt := 0, wr := A_ScreenWidth, wb := A_ScreenHeight
    loop MonitorGetCount() {
        MonitorGet(A_Index, &l, &t, &r, &b)
        if (mx >= l && mx < r && my >= t && my < b) {
            MonitorGetWorkArea(A_Index, &wl, &wt, &wr, &wb)
            break
        }
    }
    g.OnEvent('Escape', (*) => CloseKeyOverlay())
    g.OnEvent('Close', (*) => CloseKeyOverlay())
    KeyOverlay := g
    g.Show(Format('x{} y{} w{} h{}', wl + (wr - wl - w) // 2, wt + (wb - wt - h) // 2, w, h))
    DllCall('dwmapi\DwmSetWindowAttribute', 'ptr', g.Hwnd, 'int', 33, 'int*', 2, 'int', 4)  ; esquinas redondeadas
    SetTimer(KeyOverlayWatch, 300)               ; cierra al perder el foco
}

CloseKeyOverlay() {
    global KeyOverlay
    SetTimer(KeyOverlayWatch, 0)
    try KeyOverlay.Destroy()
    KeyOverlay := ''
}

KeyOverlayWatch() {
    global KeyOverlay
    if (KeyOverlay != '') && !WinActive('ahk_id ' KeyOverlay.Hwnd)
        CloseKeyOverlay()
}

; --- Lista de juegos (games.toml: exe = "...") --------------------------------
GameExes := Map()
LoadGames() {
    global GameExes
    GameExes := Map()
    if !FileExist(GamesToml)
        return
    for line in StrSplit(FileRead(GamesToml, 'UTF-8'), '`n') {
        if RegExMatch(line, 'i)^\s*exe\s*=\s*"([^"]+)"', &m) {
            exe := StrLower(m[1])
            if !InStr(exe, '*')                 ; los placeholders con * se ignoran
                GameExes[exe] := true
        }
    }
}
LoadGames()

; --- Game-mode watcher (≥1 s, costo CPU despreciable) --------------------------
GameModeActive := false
SetTimer(GameWatch, 1000)

GameWatch() {
    global GameModeActive
    active := false

    ; 1) Flag manual/CLI: winarchy game-mode on
    if FileExist(GameFlag)
        active := true

    ; 2) Proceso en primer plano listado en games.toml
    if !active {
        try {
            exe := StrLower(WinGetProcessName('A'))
            if GameExes.Has(exe) || GameExes.Has(RegExReplace(exe, '\.exe$'))
                active := true
        }
    }

    ; 3) Red de seguridad: fullscreen-exclusive (sin caption, tamaño = monitor)
    if !active {
        try {
            hwnd := WinGetID('A')
            style := WinGetStyle(hwnd)
            if !(style & 0xC00000) {            ; sin WS_CAPTION
                WinGetPos(&x, &y, &w, &h, hwnd)
                MonitorGet(MonitorGetPrimary(), &ml, &mt, &mr, &mb)
                ; cubre el monitor donde está (chequeo sobre todos los monitores)
                loop MonitorGetCount() {
                    MonitorGet(A_Index, &l, &t, &r, &b)
                    if (x <= l && y <= t && x + w >= r && y + h >= b) {
                        exe := StrLower(WinGetProcessName(hwnd))
                        ; nunca tratar shell/escritorio como juego
                        if exe != 'explorer.exe' && exe != 'searchhost.exe' && exe != 'yasb.exe'
                            active := true
                        break
                    }
                }
            }
        }
    }

    if (active && !GameModeActive) {
        GameModeActive := true
        Suspend(true)                            ; hotkeys SUPER fuera, el juego manda
        LoadGames()                              ; refresco para altas en caliente
    } else if (!active && GameModeActive) {
        GameModeActive := false
        Suspend(false)
    }
}

; --- Accent watcher (theme dinámico auto-accent) -------------------------------
; Solo activo si existe state\dynamic-accent (lo escribe `winarchy theme set` de un
; theme con dynamic_accent = true). Tick ocioso = 1 FileExist; con marker, 1 RegRead.
AccentMarker := StateDir "\dynamic-accent"
AccentPending := ''            ; debounce: 2 lecturas iguales antes de sincronizar
AccentLastSynced := ''         ; guard anti-loop si el sync falla
AccentSyncTick := 0
SetTimer(AccentWatch, 5000)

AccentWatch() {
    global AccentPending, AccentLastSynced, AccentSyncTick
    if !FileExist(AccentMarker) || GameModeActive   ; inerte sin theme dinámico o en juego
        return
    try v := RegRead('HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent', 'AccentColorMenu')
    catch
        return
    hex := Format('#{:02x}{:02x}{:02x}', v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF)
    if (hex = Trim(FileRead(AccentMarker, 'UTF-8'))) {
        AccentPending := ''
        return
    }
    ; Windows escribe el accent en varias pasadas al cambiar wallpaper: confirmar
    ; el valor en dos ticks seguidos antes de disparar nada
    if (hex != AccentPending) {
        AccentPending := hex
        return
    }
    ; si el sync no actualizó el marker (falló), no spawnear pwsh cada 5 s
    if (hex = AccentLastSynced && A_TickCount - AccentSyncTick < 60000)
        return
    AccentLastSynced := hex
    AccentSyncTick := A_TickCount
    Winarchy('accent sync')
}

; ============================================================================
; HOTKEYS — esquema SUPER (todo lo de abajo se suspende en game-mode)
; ============================================================================

; --- Apps ---------------------------------------------------------------------
#Enter::Run('wt.exe')                            ; terminal
#Space::ToggleFlow()                             ; Flow Launcher
#+Space::WinarchyTerminal('menu')                ; menú winarchy
#b::Run(DefaultBrowser())                        ; browser
#e::Run('explorer.exe')                          ; explorador de archivos
; Win+N queda libre para Windows (centro de notificaciones; la campanita de YASB lo simula)
#m::LaunchMusic()                                ; música (Spotify / YT Music)
#o::LaunchObsidian()                             ; Obsidian

; --- Webapps --------------------------------------------------------------------
#a::WebApp('https://chatgpt.com')                ; ChatGPT
#+a::WebApp('https://claude.ai')                 ; Claude
#y::WebApp('https://youtube.com')                ; YouTube
#x::WebApp('https://x.com')                      ; X
#c::WebApp('https://calendar.google.com')        ; Google Calendar
#g::WebApp('https://web.whatsapp.com')           ; WhatsApp (en juego, Win+G = Game Bar)

; --- Ventanas -------------------------------------------------------------------
#w::Komorebic('close')                           ; cerrar ventana
#f::Komorebic('toggle-monocle')                  ; monocle (fullscreen lógico)
#+f::Komorebic('toggle-maximize')                ; maximize real
#t::Komorebic('toggle-float')                    ; flotar/anclar
#p::Komorebic('toggle-pause')                    ; pausar tiling
#r::Komorebic('retile')                          ; re-tilear forzado
#+r::Winarchy('reload')                          ; recargar todo el stack

; --- Foco direccional -----------------------------------------------------------
#Left::Komorebic('focus left')                   ; mover el foco
#Right::Komorebic('focus right')
#Up::Komorebic('focus up')
#Down::Komorebic('focus down')

; --- Mover ventana en el layout ---------------------------------------------------
#+Left::Komorebic('move left')                   ; mover la ventana
#+Right::Komorebic('move right')
#+Up::Komorebic('move up')
#+Down::Komorebic('move down')

; --- Resize -----------------------------------------------------------------------
#=::Komorebic('resize-axis horizontal increase') ; ancho +
#-::Komorebic('resize-axis horizontal decrease') ; ancho -
#+=::Komorebic('resize-axis vertical increase')  ; alto +
#+-::Komorebic('resize-axis vertical decrease')  ; alto -

; --- Workspaces 1-9 (komorebi, no virtual desktops nativos) -------------------------
; Exentos del game-mode: si se suspenden, Win+N cae al atajo nativo de Windows
; (lanza la app N de la taskbar), que es peor que poder salir del juego por workspace.
#SuspendExempt
#1::Komorebic('focus-workspace 0')               ; ir al workspace N
#2::Komorebic('focus-workspace 1')
#3::Komorebic('focus-workspace 2')
#4::Komorebic('focus-workspace 3')
#5::Komorebic('focus-workspace 4')
#6::Komorebic('focus-workspace 5')
#7::Komorebic('focus-workspace 6')
#8::Komorebic('focus-workspace 7')
#9::Komorebic('focus-workspace 8')
#SuspendExempt False

#+1::Komorebic('move-to-workspace 0')            ; mover ventana al workspace N
#+2::Komorebic('move-to-workspace 1')
#+3::Komorebic('move-to-workspace 2')
#+4::Komorebic('move-to-workspace 3')
#+5::Komorebic('move-to-workspace 4')
#+6::Komorebic('move-to-workspace 5')
#+7::Komorebic('move-to-workspace 6')
#+8::Komorebic('move-to-workspace 7')
#+9::Komorebic('move-to-workspace 8')

; --- Monitores ------------------------------------------------------------------------
#,::Komorebic('cycle-focus-monitor previous')    ; foco al monitor anterior
#.::Komorebic('cycle-focus-monitor next')        ; foco al monitor siguiente
#+,::Komorebic('cycle-move-monitor previous')    ; mover al monitor anterior
#+.::Komorebic('cycle-move-monitor next')        ; mover al monitor siguiente

; --- Capturas (ShareX) -------------------------------------------------------------------
#+s::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -RectangleRegion')  ; captura de región
#+w::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -ActiveWindow')     ; captura de ventana activa
#+p::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -PrintScreen')      ; captura de pantalla completa

; --- Themes / ayuda --------------------------------------------------------------------
#+t::Winarchy('theme next')                       ; siguiente theme
#^t::Winarchy('theme gallery')                    ; galería visual de themes
#k::ToggleKeyOverlay()                            ; este overlay de keybindings

; --- Override de usuario (config\ahk\user.ahk, no versionado; sus hotkeys
;     aparecen en el overlay bajo "Usuario" o sus propios headers "; --- X ---")
#HotIf
#Include *i %A_ScriptDir%\user.ahk
