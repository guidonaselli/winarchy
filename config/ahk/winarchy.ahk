; ============================================================================
; winarchy.ahk — único dueño de hotkeys globales de Winarchy (AHK v2)
; Esquema SUPER estilo Omarchy → despacha a komorebic / Flow / Terminal / winarchy
; Absorbe el viejo win-space-launcher.ahk (Win+Space → Flow Launcher).
; ============================================================================
#Requires AutoHotkey v2.0
#SingleInstance Force
ProcessSetPriority "High"   ; el dispatcher debe responder siempre, costo ~0

; Limpia flags de sub-sesión de Claude Code heredados
EnvSet('CLAUDE_CODE_CHILD_SESSION')
EnvSet('CLAUDECODE')
EnvSet('NO_COLOR')

; --- Rutas (el script vive en <repo>\config\ahk) -----------------------------
RepoRoot := RegExReplace(A_ScriptDir, "\\config\\ahk$")
StateDir := RepoRoot "\state"
GameFlag := StateDir "\game-mode.flag"
MenuFlag := StateDir "\show-menu.flag"
AwakeFlag := StateDir "\stay-awake.flag"
GamesToml := RepoRoot "\games.toml"
WinarchyPs1 := RepoRoot "\bin\winarchy.ps1"
DirCreate(StateDir)

; Define WinarchyUserMenu con entradas propias del menu y del tray.
#Include *i %A_ScriptDir%\user-menu.ahk

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

WeztermExe := FileExist(A_ProgramFiles "\WezTerm\wezterm-gui.exe")
    ? A_ProgramFiles "\WezTerm\wezterm-gui.exe"
    : "wezterm-gui.exe"
; --config-file explicito: WEZTERM_CONFIG_FILE puede faltar en el entorno heredado (AHK
; arrancado antes de instalar WezTerm) y ahi WezTerm cae a sus defaults, sin theme ni pwsh.
Wezterm(args := '') {
    global WeztermExe, RepoRoot
    DllCall('user32\AllowSetForegroundWindow', 'int', -1)
    Run('"' WeztermExe '" --config-file "' RepoRoot '\config\wezterm\wezterm.lua"' (args ? ' ' args : ''), , , &pid)
    if (pid && hwnd := WinWait('ahk_pid ' pid, , 2)) {
        loop 10 {
            if WinActive('ahk_id ' hwnd) || !WinExist('ahk_id ' hwnd)
                break
            try WinActivate('ahk_id ' hwnd)
            Sleep(30)
        }
    }
}

Komorebic(cmd) {
    Run('"' KomorebicExe '" ' cmd, , 'Hide')
}

; WM_CLOSE to the active window, never to the desktop or taskbar.
CloseWindow() {
    if !(hwnd := WinExist('A'))
        return
    try {
        if WinGetClass(hwnd) ~= '^(Progman|WorkerW|Shell_TrayWnd|Shell_SecondaryTrayWnd)$'
            || WinGetProcessName(hwnd) = 'yasb.exe'
            return
        PostMessage(0x0010, 0, 0, , hwnd)
    }
}

; ShareX directo: como Komorebic(), evita el spawn de pwsh + Import-Module completo del
; módulo (24 archivos) que paga `winarchy screenshot` por cada hotkey de captura.
ShareXExe := FileExist(A_ProgramFiles "\ShareX\ShareX.exe")
    ? A_ProgramFiles "\ShareX\ShareX.exe"
    : (FileExist(EnvGet('ProgramFiles(x86)') "\ShareX\ShareX.exe") ? EnvGet('ProgramFiles(x86)') "\ShareX\ShareX.exe" : "")

Sharex(action) {
    global ShareXExe
    if !ShareXExe {
        TrayTip('ShareX no está instalado (winget install ShareX.ShareX)', 'Winarchy')
        return
    }
    Run('"' ShareXExe '" -' action, , 'Hide')
}

Winarchy(args) {
    DllCall('user32\AllowSetForegroundWindow', 'int', -1)
    Run('pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' WinarchyPs1 '" ' args, , 'Hide')
}

WinarchyTerminal(args) {
    Wezterm('start -- pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' WinarchyPs1 '" ' args)
}

FlowWindow := 'Flow.Launcher ahk_exe Flow.Launcher.exe'

ToggleFlow() {
    ; Reusa el hotkey nativo de Flow (Alt+Space) para mostrar/ocultar su ventana ya
    ; existente, en vez de relanzar el exe.
    global FlowWindow
    flow := EnvGet('LOCALAPPDATA') '\FlowLauncher\Flow.Launcher.exe'
    if !FileExist(flow)
        return 0
    if ProcessExist('Flow.Launcher.exe') {
        wasVisible := WinExist(FlowWindow)
        Send('!{Space}')
        if wasVisible
            return 0
        hwnd := WinWait(FlowWindow, , 2)
        return hwnd ? hwnd : 0
    }
    ; Cold start: todavía no hay proceso corriendo (recién logueado), así que tampoco
    ; hay hotkey de Flow registrado — acá sí hace falta lanzar el exe.
    Run('"' flow '"')
    ; Tras el logon Flow sigue indexando programas y tarda bastante más que los 2 s que
    ; esperábamos antes: se vencía el WinWait, no se activaba nada y la primera pulsación
    ; del día se sentía muerta.
    hwnd := WinWait(FlowWindow, , 5)
    if !hwnd
        return 0
    ; La instancia en background no siempre puede tomar el foreground (lock de Windows):
    ; reintentar la activación hasta que el foco quede realmente en el query box.
    loop 10 {
        if !WinExist('ahk_id ' hwnd)
            return 0
        try WinActivate('ahk_id ' hwnd)
        if WinActive('ahk_id ' hwnd)
            return hwnd
        Sleep(30)
    }
    return 0
}

; Flow with a plugin keyword preloaded ("app " programs, "f " files; Set-WinarchyFlowKeywords).
ToggleFlowScoped(prefix) {
    if !ToggleFlow()
        return
    Sleep(50)
    Send('^a')
    SendText(prefix)
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

; --- System / power actions (SUPER+Esc) --------------------------------------------
LockWorkstation() {
    Run('rundll32.exe user32.dll,LockWorkStation', , 'Hide')
}

SleepSystem() {
    Run('rundll32.exe powrprof.dll,SetSuspendState 0,1,0', , 'Hide')
}

HibernateSystem() {
    Run('shutdown.exe /h', , 'Hide')
}

SignOutSystem() {
    Run('shutdown.exe /l', , 'Hide')
}

RestartSystem() {
    Run('shutdown.exe /r /t 0', , 'Hide')
}

ShutdownSystem() {
    Run('shutdown.exe /s /t 0', , 'Hide')
}

LaunchScreensaver() {
    try {
        scr := RegRead('HKCU\Control Panel\Desktop', 'SCRNSAVE.EXE')
        if FileExist(scr)
            Run('"' scr '" /s')
    }
}

; --- Tray ------------------------------------------------------------------------
; Same items as the palette menu, rebuilt on every right-click.
if FileExist(RepoRoot "\assets\logo\winarchy.ico")
    try TraySetIcon(RepoRoot "\assets\logo\winarchy.ico")
A_IconTip := "Winarchy"
SetupTray()
OnMessage(0x404, TrayNotify)

TrayNotify(wParam, lParam, *) {
    if (lParam = 0x205)                  ; WM_RBUTTONUP
        SetupTray()
}

SetupTray() {
    A_TrayMenu.Delete()
    FillNativeMenu(A_TrayMenu, WinarchyMenuItemsWithUser())
    A_TrayMenu.Default := '1&'
}

FillNativeMenu(m, items) {
    for it in items {
        if it.HasOwnProp('sub') {
            sub := Menu()
            FillNativeMenu(sub, it.sub)
            m.Add(it.text, sub)
        } else
            try m.Add(it.text (it.HasOwnProp('hint') ? "`t" it.hint : ''), it.action)
    }
}

; --- Menu watcher (puente para el botón de la barra YASB) ----------------------
; YASB (proceso separado) no puede llamar funciones de AHK directamente: el click
; en el widget de la barra toca este flag (mismo patrón que GameFlag) y este
; watcher lo levanta y muestra el menú. Poll rápido porque es una acción de click,
; no algo ocioso de fondo.
SetTimer(MenuWatch, 150)

MenuWatch() {
    global MenuFlag
    if FileExist(MenuFlag) {
        try FileDelete(MenuFlag)
        ShowMainMenu()
    }
}

ToggleGameMode() {
    global GameFlag
    Winarchy(FileExist(GameFlag) ? 'game-mode off' : 'game-mode on')
}

; --- Stay awake ---------------------------------------------------------------
ToggleStayAwake() {
    global AwakeFlag
    if FileExist(AwakeFlag) {
        DllCall('SetThreadExecutionState', 'UInt', 0x80000000)   ; ES_CONTINUOUS
        FileDelete(AwakeFlag)
        TrayTip('Stay awake: off', 'Winarchy')
    } else {
        ; ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
        DllCall('SetThreadExecutionState', 'UInt', 0x80000003)
        FileAppend('', AwakeFlag)
        TrayTip('Stay awake: on', 'Winarchy')
    }
}

QuitStack() {
    ; parada ordenada y NO elevada: komorebi primero, luego barra/launcher, AHK al final
    try Komorebic('stop')
    try RunWait('taskkill /IM yasb.exe /F', , 'Hide')
    try RunWait('taskkill /IM Flow.Launcher.exe /F', , 'Hide')
    ExitApp()
}

; --- YASB watchdog --------------------------------------------------------------
; Relaunches YASB after two missed checks; paused while winget runs; gives up after 3 relaunches in 5 min.
YasbcExe := FileExist(A_ProgramFiles "\YASB\yasbc.exe") ? A_ProgramFiles "\YASB\yasbc.exe" : ""
YasbMisses := 0
YasbRelaunches := []
if YasbcExe
    SetTimer(YasbWatch, 5000)

YasbWatch() {
    global YasbMisses, YasbRelaunches
    if ProcessExist('yasb.exe') || ProcessExist('winget.exe') {
        YasbMisses := 0
        return
    }
    if (++YasbMisses < 2)
        return
    YasbMisses := 0
    while YasbRelaunches.Length && A_TickCount - YasbRelaunches[1] > 300000
        YasbRelaunches.RemoveAt(1)
    if (YasbRelaunches.Length >= 3) {
        SetTimer(YasbWatch, 0)
        TrayTip('The bar keeps crashing; stopped relaunching it. Run: winarchy doctor', 'Winarchy')
        return
    }
    YasbRelaunches.Push(A_TickCount)
    EnvSet('YASB_CONFIG_HOME', RepoRoot '\config\yasb')
    Run('"' YasbcExe '" start', , 'Hide')
}

; --- Menu items (palette menu and tray share them) ---------------------------------
OnOff(flag) => Chr(0xB7) ' ' (FileExist(flag) ? 'on' : 'off')

WinarchyMenuItems() {
    global GameFlag, AwakeFlag
    return [
        {text:'Apps',          hint:'SUPER+Space',        action:(*)=>ToggleFlowScoped('app ')},
        {text:'Themes',                                   sub: WinarchyThemeItems()},
        {text:'Capture',                                  sub: WinarchyCaptureItems()},
        {text:'Tiling',                                   sub: WinarchyTilingItems()},
        {text:'Bar',                                      sub: WinarchyBarItems()},
        {text:'Coding agent',  hint:'SUPER+Ctrl+Shift+A', action:(*)=>Winarchy('agent launch')},
        {text:'Keybindings',   hint:'SUPER+K',            action:(*)=>ToggleKeyOverlay()},
        {text:'Reload stack',  hint:'SUPER+Shift+R',      action:(*)=>Winarchy('reload')},
        {text:'Game mode ' OnOff(GameFlag),               action:(*)=>ToggleGameMode()},
        {text:'Stay awake ' OnOff(AwakeFlag), hint:'SUPER+Ctrl+W', action:(*)=>ToggleStayAwake()},
        {text:'Doctor',                                   action:(*)=>WinarchyTerminal('doctor')},
        {text:'Check for updates',                        action:(*)=>WinarchyTerminal('update')},
        {text:'System',        hint:'SUPER+Esc',          sub: WinarchySysItems()},
        {text:'Quit Winarchy',                            action:(*)=>QuitStack()} ]
}

WinarchyMenuItemsWithUser() {
    items := WinarchyMenuItems()
    if IsSet(WinarchyUserMenu) && WinarchyUserMenu is Array {
        for entry in WinarchyUserMenu
            items.InsertAt(items.Length, entry)     ; before Quit, which closes the list
    }
    return items
}

WinarchyThemeItems() {
    return [
        {text:'Next theme',      hint:'SUPER+Shift+T',    action:(*)=>Winarchy('theme next')},
        {text:'Theme gallery',   hint:'SUPER+Ctrl+T',     action:(*)=>Winarchy('theme gallery')},
        {text:'Next background', hint:'SUPER+Ctrl+Space', action:(*)=>Winarchy('background next')} ]
}

WinarchyTilingItems() {
    return [
        {text:'Manage this window',                                action:(*)=>Komorebic('manage')},
        {text:'Unmanage this window',                              action:(*)=>Komorebic('unmanage')},
        {text:'Stop tiling this workspace', hint:'SUPER+Shift+Z',  action:(*)=>Komorebic('toggle-tiling')},
        {text:'New windows: stack / tile',                         action:(*)=>Komorebic('toggle-window-container-behaviour')},
        {text:'Title bars',                                        action:(*)=>Komorebic('toggle-title-bars')},
        {text:'Mouse follows focus',                               action:(*)=>Komorebic('toggle-mouse-follows-focus')},
        {text:'Restore hidden windows',                            action:(*)=>Komorebic('restore-windows')} ]
}

WinarchyCaptureItems() {
    return [
        {text:'Region',                 hint:'SUPER+Shift+S', action:(*)=>Winarchy('screenshot region')},
        {text:'Window',                 hint:'SUPER+Shift+W', action:(*)=>Winarchy('screenshot window')},
        {text:'Full screen',            hint:'SUPER+Shift+P', action:(*)=>Winarchy('screenshot full')},
        {text:'Repeat last region',                           action:(*)=>Winarchy('screenshot last')},
        {text:'Scrolling capture',                            action:(*)=>Winarchy('screenshot scrolling')},
        {text:'Record screen',          hint:'SUPER+Shift+V', action:(*)=>Winarchy('screenshot record')},
        {text:'Record as GIF',          hint:'SUPER+Shift+G', action:(*)=>Winarchy('screenshot record-gif')},
        {text:'Stop recording',         hint:'SUPER+Ctrl+V',  action:(*)=>Winarchy('screenshot stop')},
        {text:'Text from screen (OCR)', hint:'SUPER+Ctrl+O',  action:(*)=>Winarchy('screenshot ocr')},
        {text:'Scan QR code',           hint:'SUPER+Ctrl+Q',  action:(*)=>Winarchy('screenshot qr')},
        {text:'Color picker',                                 action:(*)=>Winarchy('screenshot color')},
        {text:'Pin to screen',                                action:(*)=>Winarchy('screenshot pin')},
        {text:'Ruler',                                        action:(*)=>Winarchy('screenshot ruler')} ]
}

WinarchyBarItems() {
    return [
        {text:'Move to top',          action:(*)=>Winarchy('bar position top')},
        {text:'Move to bottom',       action:(*)=>Winarchy('bar position bottom')},
        {text:'Toggle transparency',  action:(*)=>Winarchy('bar transparent')} ]
}

WinarchySysItems() {
    return [
        {text:'Lock',            action:(*)=>LockWorkstation()},
        {text:'Screensaver',     action:(*)=>LaunchScreensaver()},
        {text:'Sleep',           action:(*)=>SleepSystem()},
        {text:'Hibernate',       action:(*)=>HibernateSystem()},
        {text:'Sign out',        action:(*)=>SignOutSystem()},
        {text:'Restart',         action:(*)=>RestartSystem()},
        {text:'Shut down',       action:(*)=>ShutdownSystem()},
        {text:'Settings',        action:(*)=>Run('explorer.exe ms-settings:')} ]
}

ShowMainMenu(*) {
    if !PalClosed('menu')
        PalOpen('menu', WinarchyMenuItemsWithUser(), 'Winarchy')
}

ShowSystemMenu(*) {
    if !PalClosed('system')
        PalOpen('system', WinarchySysItems(), 'System')
}

ToggleKeyOverlay(*) {
    if PalClosed('keys')
        return
    sections := ParseKeymap(A_ScriptFullPath, '', '; HOTKEYS')
    if FileExist(A_ScriptDir '\user.ahk')
        for s in ParseKeymap(A_ScriptDir '\user.ahk', 'User')
            sections.Push(s)
    PalOpen('keys', sections, 'Keybindings')
}

; Sections come from the "; --- Title ---" headers; descriptions from the inline comment.
ParseKeymap(path, defaultTitle := '', startAt := '') {
    sections := [], cur := '', on := (startAt = '')
    if (defaultTitle != '') {
        cur := {title: defaultTitle, items: []}
        sections.Push(cur)
    }
    for line in StrSplit(FileRead(path, 'UTF-8'), '`n') {
        line := RTrim(line, '`r')
        if !on {
            on := InStr(line, startAt) = 1
            continue
        }
        if RegExMatch(line, '^; --- (.+?) -{2,}', &m) {
            cur := {title: RegExReplace(Trim(m[1]), '\s*\(.*\)$'), items: []}
            sections.Push(cur)
            continue
        }
        if !IsObject(cur) || !RegExMatch(line, '^#([+^!]*)([^:\s]+)::(.*)$', &m)
            continue
        mods := m[1], key := m[2], rest := m[3]
        ; 1..9 and the four arrows are listed once
        if RegExMatch(key, '^[2-9]$') || key = 'Right' || key = 'Up' || key = 'Down'
            continue
        if (key = '1')
            key := '1' Chr(0x2026) '9'
        else if (key = 'Left')
            key := Chr(0x2190) ' ' Chr(0x2191) ' ' Chr(0x2192) ' ' Chr(0x2193)
        else if (key = 'Enter')
            key := 'Return'
        else
            key := StrUpper(SubStr(key, 1, 1)) SubStr(key, 2)
        desc := RegExMatch(rest, ';\s*(.+)$', &md) ? Trim(md[1]) : Trim(rest)
        cur.items.Push({
            keys: 'SUPER' (InStr(mods, '!') ? '+Alt' : '') (InStr(mods, '^') ? '+Ctrl' : '') (InStr(mods, '+') ? '+Shift' : '') '+' key,
            desc: StrUpper(SubStr(desc, 1, 1)) SubStr(desc, 2)})
    }
    return sections
}

; --- Palette: themed, searchable popup list (menu, system menu, SUPER+K) -----------
Pal := ''

ThemeColors() {
    c := {bg:'1a1b26', fg:'c0caf5', ac:'7aa2f7', mut:'565f89'}
    ini := A_ScriptDir '\theme.ini'
    if FileExist(ini) {
        c.bg  := LTrim(IniRead(ini, 'colors', 'background', c.bg), '#')
        c.fg  := LTrim(IniRead(ini, 'colors', 'foreground', c.fg), '#')
        c.ac  := LTrim(IniRead(ini, 'colors', 'accent', c.ac), '#')
        c.mut := LTrim(IniRead(ini, 'colors', 'muted', c.mut), '#')
    }
    return c
}

; Closes any open palette; true when it was showing `mode` (the hotkey toggles it).
PalClosed(mode) {
    global Pal
    if !IsObject(Pal)
        return false
    same := (Pal.mode = mode)
    PalClose()
    return same
}

PalOpen(mode, items, title) {
    global Pal
    c := ThemeColors()
    keys := (mode = 'keys')
    pad := 20, rowH := keys ? 28 : 32
    labelW := keys ? 250 : 330, hintW := keys ? 470 : 190
    w := pad * 2 + labelW + hintW
    n := keys ? 18 : Min(Max(items.Length, 8), 14)

    g := Gui('-Caption +AlwaysOnTop +ToolWindow', 'WinarchyPalette')
    g.BackColor := c.bg
    g.MarginX := 0, g.MarginY := 0

    PalSetFont(g, 's10 bold')
    crumb := g.Add('Text', Format('x{} y{} w{} h20 c{} 0x4200', pad, pad, w - pad * 2, c.ac), '')
    PalSetFont(g, 's12 norm')
    g.Add('Text', Format('x{} y{} w24 h30 c{} 0x200', pad, pad + 30, c.mut), Chr(0xF002))
    ed := g.Add('Edit', Format('x{} y{} w{} h30 -E0x200 -Multi Background{} c{}', pad + 28, pad + 34, w - pad * 2 - 28, c.bg, c.fg))
    SendMessage(0x1501, 1, StrPtr(keys ? 'Filter by key or action' : 'Search'), ed)   ; EM_SETCUEBANNER
    g.Add('Text', Format('x{} y{} w{} h1 Background{}', pad, pad + 68, w - pad * 2, c.mut))

    y0 := pad + 80
    PalSetFont(g, 's11 norm')
    rows := []
    loop n {
        y := y0 + (A_Index - 1) * rowH
        lbl := g.Add('Text', Format('x{} y{} w{} h{} c{} Background{} 0x4200', pad - 8, y, labelW + 8, rowH, c.fg, c.bg), '')
        hnt := g.Add('Text', Format('x{} y{} w{} h{} c{} Background{} 0x4200 {}', pad + labelW, y, hintW + 8, rowH, c.mut, c.bg, keys ? '' : 'Right'), '')
        rows.Push({label: lbl, hint: hnt, y: y, on: false})
    }
    fy := y0 + n * rowH + 10
    PalSetFont(g, 's9 norm')
    arrows := Chr(0x2191) Chr(0x2193)
    help := keys ? arrows ' scroll    esc close' : arrows ' move    ' Chr(0x21B5) ' select    esc back'
    g.Add('Text', Format('x{} y{} w{} h20 c{}', pad, fy, labelW, c.mut), help)
    count := g.Add('Text', Format('x{} y{} w{} h20 c{} Right', pad + labelW, fy, hintW, c.mut), '')
    h := fy + 20 + pad - 4

    Pal := {gui: g, mode: mode, colors: c, items: items, title: title, stack: [], list: [],
        sel: 0, top: 1, rows: rows, rowH: rowH, y0: y0, x0: pad - 8, x1: pad + labelW + hintW + 8,
        edit: ed, crumb: crumb, count: count, mouse: ''}
    ed.OnEvent('Change', (*) => PalRefresh())
    g.OnEvent('Close', (*) => PalClose())
    PalRefresh()

    WorkAreaUnderMouse(&wl, &wt, &wr, &wb)
    g.Show(Format('x{} y{} w{} h{}', wl + (wr - wl - w) // 2, wt + (wb - wt - h) // 2, w, h))
    DllCall('dwmapi\DwmSetWindowAttribute', 'ptr', g.Hwnd, 'int', 33, 'int*', 2, 'int', 4)  ; rounded corners
    ed.Focus()
    OnMessage(0x200, PalMouseMove)       ; WM_MOUSEMOVE
    OnMessage(0x202, PalClick)           ; WM_LBUTTONUP
    OnMessage(0x20A, PalWheel)           ; WM_MOUSEWHEEL
    SetTimer(PalWatch, 250)              ; closes when it loses focus
}

; JetBrainsMono under its Nerd Fonts v2 or v3 family name.
PalSetFont(g, opts) {
    g.SetFont(opts, 'JetBrainsMono Nerd Font')
    g.SetFont(opts, 'JetBrainsMono NF')
}

PalClose() {
    global Pal
    SetTimer(PalWatch, 0)
    OnMessage(0x200, PalMouseMove, 0)
    OnMessage(0x202, PalClick, 0)
    OnMessage(0x20A, PalWheel, 0)
    if IsObject(Pal)
        try Pal.gui.Destroy()
    Pal := ''
}

PalWatch() {
    try {
        if IsObject(Pal) && !WinActive('ahk_id ' Pal.gui.Hwnd)
            PalClose()
    } catch
        PalClose()
}

PalActive() => IsObject(Pal) && WinActive('ahk_id ' Pal.gui.Hwnd)

PalMatch(q, hay) {
    for word in StrSplit(q, ' ')
        if (word != '' && !InStr(hay, word))
            return false
    return true
}

PalHint(it) => it.HasOwnProp('sub') ? Chr(0x203A) : (it.HasOwnProp('hint') ? it.hint : '')

; Leaves of the tree whose path matches, e.g. "Capture › Region".
PalFlatten(items, path, q, list) {
    for it in items {
        name := (path = '') ? it.text : path ' ' Chr(0x203A) ' ' it.text
        if it.HasOwnProp('sub')
            PalFlatten(it.sub, name, q, list)
        else if PalMatch(q, name ' ' PalHint(it))
            list.Push({text: name, hint: PalHint(it), item: it})
    }
}

PalRefresh(sel := 0) {
    q := Trim(Pal.edit.Value)
    list := []
    if (Pal.mode = 'keys') {
        for s in Pal.items {
            hits := []
            for it in s.items
                if PalMatch(q, it.keys ' ' it.desc ' ' s.title)
                    hits.Push({text: it.keys, hint: it.desc})
            if hits.Length {
                list.Push({text: StrUpper(s.title), hint: '', header: true})
                for e in hits
                    list.Push(e)
            }
        }
    } else if (q = '') {
        for it in Pal.items
            list.Push({text: it.text, hint: PalHint(it), item: it})
    } else
        PalFlatten(Pal.items, '', q, list)
    Pal.list := list
    Pal.top := 1
    Pal.sel := 0
    for i, e in list
        if !e.HasOwnProp('header') && (!Pal.sel || i = sel) {
            Pal.sel := i
            if !sel
                break
        }
    crumb := StrUpper(Pal.title)
    for lvl in Pal.stack
        crumb := StrUpper(lvl.title) ' ' Chr(0x203A) ' ' crumb
    Pal.crumb.Text := crumb
    found := 0
    for e in list
        found += !e.HasOwnProp('header')
    Pal.count.Text := (Pal.mode = 'keys') ? found ' bindings' : (q = '' ? '' : found ' results')
    PalDraw()
}

PalDraw() {
    c := Pal.colors, n := Pal.rows.Length, len := Pal.list.Length
    if Pal.sel {
        if (Pal.sel < Pal.top)
            Pal.top := Pal.sel
        else if (Pal.sel > Pal.top + n - 1)
            Pal.top := Pal.sel - n + 1
        if (Pal.top = Pal.sel && Pal.sel > 1 && Pal.list[Pal.sel - 1].HasOwnProp('header'))
            Pal.top -= 1                 ; keep the section title above its first row
    }
    Pal.top := Max(1, Min(Pal.top, len - n + 1))
    for k, r in Pal.rows {
        i := Pal.top + k - 1
        e := (i <= len) ? Pal.list[i] : {text: (k = 1 && !len) ? 'No matches' : '', hint: '', empty: true}
        head := e.HasOwnProp('header'), on := (i = Pal.sel)
        if (on != r.on) {
            r.on := on
            r.label.Opt('Background' (on ? c.ac : c.bg))
            r.hint.Opt('Background' (on ? c.ac : c.bg))
        }
        r.label.SetFont((head ? 'bold' : 'norm') ' c' (on ? c.bg : head ? c.ac : e.HasOwnProp('empty') ? c.mut : c.fg))
        r.hint.SetFont('c' (on ? c.bg : Pal.mode = 'keys' ? c.fg : c.mut))
        r.label.Text := ' ' e.text
        r.hint.Text := e.hint ' '
    }
}

PalMove(d) {
    if !Pal.sel
        return
    len := Pal.list.Length, i := Pal.sel, step := (d > 0) ? 1 : -1
    loop Abs(d) {
        j := i
        loop {
            j += step
            if (j < 1 || j > len) {
                if (Abs(d) > 1)          ; page / wheel: stop at the ends
                    break 2
                j := (j < 1) ? len : 1
            }
            if !Pal.list[j].HasOwnProp('header')
                break
        }
        i := j
    }
    Pal.sel := i
    PalDraw()
}

PalEnter() {
    if !Pal.sel || !Pal.list[Pal.sel].HasOwnProp('item')
        return
    it := Pal.list[Pal.sel].item
    if it.HasOwnProp('sub') {
        Pal.stack.Push({items: Pal.items, title: Pal.title, sel: Pal.sel})
        Pal.items := it.sub, Pal.title := it.text
        Pal.edit.Value := ''
        PalRefresh()
        return
    }
    PalClose()
    it.action.Call()
}

PalBack() {
    if !Pal.stack.Length
        return
    prev := Pal.stack.Pop()
    Pal.items := prev.items, Pal.title := prev.title
    PalRefresh(prev.sel)
}

PalEscape() {
    if (Pal.edit.Value != '') {
        Pal.edit.Value := ''
        PalRefresh()
    } else if Pal.stack.Length
        PalBack()
    else
        PalClose()
}

; List index under the cursor, or 0.
PalIndexAtCursor(hwnd) {
    if !IsObject(Pal) || DllCall('GetAncestor', 'ptr', hwnd, 'uint', 2, 'ptr') != Pal.gui.Hwnd
        return 0
    pt := Buffer(8)
    DllCall('GetCursorPos', 'ptr', pt)
    DllCall('ScreenToClient', 'ptr', Pal.gui.Hwnd, 'ptr', pt)
    x := NumGet(pt, 0, 'int') * 96 / A_ScreenDPI, y := NumGet(pt, 4, 'int') * 96 / A_ScreenDPI
    k := Floor((y - Pal.y0) / Pal.rowH) + 1
    if (x < Pal.x0 || x > Pal.x1 || k < 1 || k > Pal.rows.Length)
        return 0
    i := Pal.top + k - 1
    return (i <= Pal.list.Length && !Pal.list[i].HasOwnProp('header')) ? i : 0
}

PalMouseMove(wParam, lParam, msg, hwnd) {
    if !IsObject(Pal) || (lParam = Pal.mouse)    ; ignore synthetic moves after a redraw
        return
    Pal.mouse := lParam
    if (i := PalIndexAtCursor(hwnd)) && (i != Pal.sel) {
        Pal.sel := i
        PalDraw()
    }
}

PalClick(wParam, lParam, msg, hwnd) {
    if (i := PalIndexAtCursor(hwnd)) {
        Pal.sel := i
        PalDraw()
        PalEnter()
    }
}

PalWheel(wParam, lParam, msg, hwnd) {
    if !IsObject(Pal) || DllCall('GetAncestor', 'ptr', hwnd, 'uint', 2, 'ptr') != Pal.gui.Hwnd
        return
    delta := (wParam >> 16) & 0xFFFF
    PalMove(delta > 0x7FFF ? 3 : -3)
    return 0
}

#HotIf PalActive()
Up::PalMove(-1)
Down::PalMove(1)
Tab::PalMove(1)
+Tab::PalMove(-1)
PgUp::PalMove(-Pal.rows.Length)
PgDn::PalMove(Pal.rows.Length)
Enter::PalEnter()
NumpadEnter::PalEnter()
Esc::PalEscape()
#HotIf PalActive() && Pal.edit.Value = ''
Backspace::PalBack()
#HotIf

WorkAreaUnderMouse(&wl, &wt, &wr, &wb) {
    CoordMode('Mouse', 'Screen')
    MouseGetPos(&mx, &my)
    wl := 0, wt := 0, wr := A_ScreenWidth, wb := A_ScreenHeight
    loop MonitorGetCount() {
        MonitorGet(A_Index, &l, &t, &r, &b)
        if (mx >= l && mx < r && my >= t && my < b) {
            MonitorGetWorkArea(A_Index, &wl, &wt, &wr, &wb)
            return
        }
    }
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

    ; NO se suspenden los hotkeys en game-mode: igual que la tecla Windows, siguen
    ; vivos durante el juego. game-mode NO pausa komorebi (solo flota la ventana del
    ; juego vía ignore-rule), así que el tiling y SUPER+1/2 siguen funcionando; los
    ; combos de geometría aplican al resto de ventanas tileadas. game-mode solo
    ; trackea estado y refresca games.toml. Animaciones ya apagadas siempre (config).
    if (active && !GameModeActive) {
        GameModeActive := true
        LoadGames()                              ; refresco para altas en caliente
    } else if (!active && GameModeActive) {
        GameModeActive := false
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
; HOTKEYS — SUPER scheme (still live in game mode, like the Windows key).
; SUPER+K lists them: "; --- Title ---" is a section, the inline comment the action.
; ============================================================================

; --- Apps ---------------------------------------------------------------------
#Enter::Wezterm()                                ; terminal
#Space::ToggleFlow()                             ; launcher (Flow)
#s::ToggleFlowScoped('f ')                       ; search files (Everything)
#b::Run(DefaultBrowser())                        ; browser
#e::Run('explorer.exe')                          ; file explorer
; Win+N stays with Windows (notification center; the YASB bell mirrors it)
#m::LaunchMusic()                                ; music (Spotify / YT Music)
#o::LaunchObsidian()                             ; Obsidian
#^+a::Winarchy('agent launch')                   ; coding agent

; --- Menus ----------------------------------------------------------------------
#!Space::ShowMainMenu()                          ; Winarchy menu
#Esc::ShowSystemMenu()                           ; system / power menu
#k::ToggleKeyOverlay()                           ; this keybindings list

; --- Webapps --------------------------------------------------------------------
#a::WebApp('https://chatgpt.com')                ; ChatGPT
#+a::WebApp('https://claude.ai')                 ; Claude
#y::WebApp('https://youtube.com')                ; YouTube
#x::WebApp('https://x.com')                      ; X
#c::WebApp('https://calendar.google.com')        ; Google Calendar
#g::WebApp('https://web.whatsapp.com')           ; WhatsApp

; --- Windows -------------------------------------------------------------------
#w::CloseWindow()                                ; close window
#f::Komorebic('toggle-monocle')                  ; monocle (fill the workspace)
#+f::Komorebic('toggle-maximize')                ; maximize
#t::Komorebic('toggle-float')                    ; float / tile
#p::Komorebic('toggle-pause')                    ; pause tiling
#r::Komorebic('retile')                          ; retile
#+r::Winarchy('reload')                          ; reload the whole stack
#+Enter::Komorebic('promote')                    ; promote to the largest tile
#+l::Komorebic('cycle-layout next')              ; next layout
#Tab::Komorebic('focus-last-workspace')          ; previous workspace
#+Tab::Komorebic('move-to-last-workspace')       ; move window to the previous workspace
#^Enter::Komorebic('promote-focus')              ; focus the largest tile
#+h::Komorebic('flip-layout horizontal')         ; mirror layout left / right
#+j::Komorebic('flip-layout vertical')           ; mirror layout up / down
#+z::Komorebic('toggle-tiling')                  ; stop tiling this workspace
#!Home::Komorebic('quick-save-resize')           ; save tile sizes
#Home::Komorebic('quick-load-resize')            ; restore tile sizes
#+d::Komorebic('toggle-transparency')            ; dim unfocused windows
#^w::ToggleStayAwake()                           ; stay awake
#^m::Komorebic('minimize')                       ; minimize
#^f::Komorebic('toggle-workspace-layer')         ; switch tiling / floating layer
#^l::Komorebic('toggle-lock')                    ; lock the tile in place

; --- Focus ---------------------------------------------------------------------
#Left::Komorebic('focus left')                   ; move focus
#Right::Komorebic('focus right')
#Up::Komorebic('focus up')
#Down::Komorebic('focus down')

; --- Move window ---------------------------------------------------------------
#+Left::Komorebic('move left')                   ; move window
#+Right::Komorebic('move right')
#+Up::Komorebic('move up')
#+Down::Komorebic('move down')

; --- Stacks (tabs in komorebi's stackbar) ---------------------------------------
#!Left::Komorebic('stack left')                  ; stack with the neighbour
#!Right::Komorebic('stack right')
#!Up::Komorebic('stack up')
#!Down::Komorebic('stack down')
#!u::Komorebic('unstack')                        ; take the window out of the stack
#^s::Komorebic('stack-all')                      ; stack the whole workspace
#^u::Komorebic('unstack-all')                    ; unstack the whole container
#!,::Komorebic('cycle-stack previous')           ; previous tab
#!.::Komorebic('cycle-stack next')               ; next tab

; --- Resize --------------------------------------------------------------------
#=::Komorebic('resize-axis horizontal increase') ; wider
#-::Komorebic('resize-axis horizontal decrease') ; narrower
#+=::Komorebic('resize-axis vertical increase')  ; taller
#+-::Komorebic('resize-axis vertical decrease')  ; shorter

; --- Workspaces (komorebi, not Windows virtual desktops) -------------------------
#1::Komorebic('focus-workspace 0')               ; go to workspace
#2::Komorebic('focus-workspace 1')
#3::Komorebic('focus-workspace 2')
#4::Komorebic('focus-workspace 3')
#5::Komorebic('focus-workspace 4')
#6::Komorebic('focus-workspace 5')
#7::Komorebic('focus-workspace 6')
#8::Komorebic('focus-workspace 7')
#9::Komorebic('focus-workspace 8')

#+1::Komorebic('move-to-workspace 0')            ; move window to workspace
#+2::Komorebic('move-to-workspace 1')
#+3::Komorebic('move-to-workspace 2')
#+4::Komorebic('move-to-workspace 3')
#+5::Komorebic('move-to-workspace 4')
#+6::Komorebic('move-to-workspace 5')
#+7::Komorebic('move-to-workspace 6')
#+8::Komorebic('move-to-workspace 7')
#+9::Komorebic('move-to-workspace 8')

#^1::Komorebic('send-to-workspace 0')            ; send window to workspace, stay here
#^2::Komorebic('send-to-workspace 1')
#^3::Komorebic('send-to-workspace 2')
#^4::Komorebic('send-to-workspace 3')
#^5::Komorebic('send-to-workspace 4')
#^6::Komorebic('send-to-workspace 5')
#^7::Komorebic('send-to-workspace 6')
#^8::Komorebic('send-to-workspace 7')
#^9::Komorebic('send-to-workspace 8')

; --- Monitors ------------------------------------------------------------------
#,::Komorebic('cycle-focus-monitor previous')    ; focus previous monitor
#.::Komorebic('cycle-focus-monitor next')        ; focus next monitor
#+,::Komorebic('cycle-move-monitor previous')    ; move window to previous monitor
#+.::Komorebic('cycle-move-monitor next')        ; move window to next monitor
#^,::Komorebic('cycle-send-to-monitor previous') ; send window to previous monitor, stay here
#^.::Komorebic('cycle-send-to-monitor next')     ; send window to next monitor, stay here
#!+,::Komorebic('cycle-move-workspace-to-monitor previous')  ; move workspace to previous monitor
#!+.::Komorebic('cycle-move-workspace-to-monitor next')      ; move workspace to next monitor

; --- Capture (ShareX) ------------------------------------------------------------
#+s::Sharex('RectangleRegion')                   ; region
#+w::Sharex('ActiveWindow')                      ; active window
#+p::Sharex('PrintScreen')                       ; full screen
#+v::Sharex('ScreenRecorder')                    ; record screen
#+g::Sharex('ScreenRecorderGIF')                 ; record as GIF
#^v::Sharex('StopScreenRecording')               ; stop recording
#^o::Sharex('OCR')                               ; text from screen (OCR)
#^q::Sharex('QRCodeScanRegion')                  ; scan QR code

; --- Themes --------------------------------------------------------------------
#+t::Winarchy('theme next')                      ; next theme
#^t::Winarchy('theme gallery')                   ; theme gallery
#^Space::Winarchy('background next')             ; next background

; --- System settings -------------------------------------------------------------
#^a::Run('explorer.exe ms-settings:sound')       ; sound
#^b::Run('explorer.exe ms-settings:bluetooth')   ; bluetooth
#^n::Run('explorer.exe ms-settings:network')     ; network
#^d::Run('explorer.exe ms-settings:display')     ; display
#^p::Run('explorer.exe ms-settings:powersleep')  ; power & sleep

; User overrides: config\ahk\user.ahk (untracked). Its hotkeys show in SUPER+K under
; "User", or under their own "; --- Title ---" headers.
#HotIf
#Include *i %A_ScriptDir%\user.ahk
