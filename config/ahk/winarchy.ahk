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
MenuFlag := StateDir "\show-menu.flag"
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

FlowWindow := 'Flow.Launcher ahk_exe Flow.Launcher.exe'

ToggleFlow() {
    ; Flow Launcher es single-instance: relanzarlo togglea la ventana de búsqueda. No
    ; expone CLI ni IPC para togglear, así que relanzar el exe es la única vía.
    ; Devuelve el hwnd con el foco puesto, o 0 si esta pulsación ocultó la ventana.
    global FlowWindow
    flow := EnvGet('LOCALAPPDATA') '\FlowLauncher\Flow.Launcher.exe'
    if !FileExist(flow)
        return 0
    ; Si la ventana ya está visible, esta pulsación la oculta: no hay nada que esperar
    ; ni activar, y quedarnos en el WinWait bloquearía el hotkey el timeout entero.
    closing := WinExist(FlowWindow)
    Run('"' flow '"')
    if closing
        return 0
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

ToggleFlowApps() {
    ; Como ToggleFlow(), pero precarga "app " en el query box: Set-WinarchyFlowAppsKeyword
    ; (Identity.ps1) agrega "app" como ActionKeyword extra del plugin Program de Flow, así
    ; que esto queda scoped a solo programas instalados — paridad con el Apps de Walker en
    ; Omarchy, sin curar una lista a mano.
    if !ToggleFlow()
        return
    Sleep(50)
    Send('^a')
    SendText('app ')
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

; --- System / power menu (SUPER+Esc, estilo Omarchy) ---------------------------
; Acciones directas vía shutdown.exe/rundll32, sin depender del menú de power
; de YASB (ese sigue andando por click en el widget "home"; esto es el atajo
; global). SysMenu se referencia tanto desde el hotkey como submenú del menú
; principal (más abajo).
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

SysMenu := Menu()
SysMenu.Add("Settings...", (*) => Run('explorer.exe ms-settings:'))
SysMenu.Add()
SysMenu.Add("Screensaver", (*) => LaunchScreensaver())
SysMenu.Add("Lock", (*) => LockWorkstation())
SysMenu.Add("Sleep", (*) => SleepSystem())
SysMenu.Add("Hibernate", (*) => HibernateSystem())
SysMenu.Add("Sign out", (*) => SignOutSystem())
SysMenu.Add("Restart", (*) => RestartSystem())
SysMenu.Add("Shut down", (*) => ShutdownSystem())

; --- Tray único de Winarchy ---------------------------------------------------
; AHK es el host del tray del stack: su icono ES la identidad de Winarchy. Los
; demás componentes ocultan su tray propio (YASB, Flow); komorebi no tiene tray.
; El tray ES el menú principal estilo Omarchy: mismo contenido vía right-click,
; SUPER+Alt+Space (hotkey) o el botón a la derecha de la barra YASB (MenuWatch).
SetupTray()

SetupTray() {
    global RepoRoot, SysMenu
    ico := RepoRoot "\assets\logo\winarchy.ico"
    if FileExist(ico)
        try TraySetIcon(ico)            ; si falla, queda el icono por defecto de AHK
    A_IconTip := "Winarchy"

    tray := A_TrayMenu
    tray.Delete()                        ; quita el menú por defecto de AHK (Pause/Suspend/Reload/Edit)

    themes := Menu()
    themes.Add("Next theme`tSUPER+Shift+T", (*) => Winarchy('theme next'))
    themes.Add("Gallery`tSUPER+Ctrl+T", (*) => Winarchy('theme gallery'))
    themes.Add("Next background`tSUPER+Ctrl+Space", (*) => Winarchy('background next'))

    tray.Add("Apps", (*) => ToggleFlowApps())
    tray.Default := "Apps"
    tray.Add()
    tray.Add("Themes", themes)
    tray.Add("Reload stack`tSUPER+Shift+R", (*) => Winarchy('reload'))
    tray.Add("Game mode (toggle)", (*) => ToggleGameMode())
    tray.Add("Doctor", (*) => WinarchyTerminal('doctor'))
    tray.Add("Check for updates", (*) => WinarchyTerminal('update'))
    tray.Add()
    tray.Add("System`tSUPER+Esc", SysMenu)
    tray.Add()
    tray.Add("Quit Winarchy", (*) => QuitStack())
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

QuitStack() {
    ; parada ordenada y NO elevada: komorebi primero, luego barra/launcher, AHK al final
    try Komorebic('stop')
    try RunWait('taskkill /IM yasb.exe /F', , 'Hide')
    try RunWait('taskkill /IM Flow.Launcher.exe /F', , 'Hide')
    ExitApp()
}

; --- Menú principal estilo Omarchy/Walker (GUI propia, centrada y tematizada) ----
; Reemplaza el menú nativo de Windows (A_TrayMenu) en el hotkey SUPER+Alt+Space y en
; el botón de la barra YASB: lista vertical centrada en el monitor activo, con los
; colores del theme actual (config\ahk\theme.ini), navegable con ↑/↓/Enter/Esc y
; click, y drill-in a submenús (Themes, System). Mismo patrón que el overlay de
; keybindings (SUPER+K). El right-click del tray sigue usando A_TrayMenu nativo: ese
; menú lo dibuja Windows y no se puede tematizar (fallback igualmente funcional).
MainMenu := ''        ; Gui activa, '' = cerrado
MenuRows := []        ; [{label, hint, y}] controles Text por fila + su Y
MenuItems := []       ; items del nivel actual
MenuSel := 1          ; índice seleccionado (1-based)
MenuTitle := ''       ; título del nivel actual
MenuStack := []       ; pila para volver de submenús
MenuSelBar := ''         ; barra de selección (un control que se mueve a la fila activa)
MenuColors := {bg:'16121E', fg:'C8C0D8', ac:'9BE53E', mut:'4A4060'}

WinarchyMenuItems() {
    global GameFlag
    return [
        {text:'Apps',          hint:'SUPER+Space',   action:(*)=>ToggleFlowApps()},
        {text:'Themes',        sub:[
            {text:'Next theme', hint:'SUPER+Shift+T', action:(*)=>Winarchy('theme next')},
            {text:'Gallery',    hint:'SUPER+Ctrl+T',  action:(*)=>Winarchy('theme gallery')},
            {text:'Next background', hint:'SUPER+Ctrl+Space', action:(*)=>Winarchy('background next')} ]},
        {text:'Reload stack',  hint:'SUPER+Shift+R', action:(*)=>Winarchy('reload')},
        {text:'Game mode',     hint:(FileExist(GameFlag) ? 'ON' : 'OFF'), action:(*)=>ToggleGameMode()},
        {text:'Doctor',                              action:(*)=>WinarchyTerminal('doctor')},
        {text:'Check updates',                       action:(*)=>WinarchyTerminal('update')},
        {text:'System',        hint:'SUPER+Esc',     sub: WinarchySysItems()},
        {text:'Quit Winarchy',                       action:(*)=>QuitStack()} ]
}

WinarchySysItems() {
    return [
        {text:'Settings…',                       action:(*)=>Run('explorer.exe ms-settings:')},
        {text:'Screensaver',                     action:(*)=>LaunchScreensaver()},
        {text:'Lock',                            action:(*)=>LockWorkstation()},
        {text:'Sleep',                           action:(*)=>SleepSystem()},
        {text:'Hibernate',                       action:(*)=>HibernateSystem()},
        {text:'Sign out',                        action:(*)=>SignOutSystem()},
        {text:'Restart',                         action:(*)=>RestartSystem()},
        {text:'Shut down',                       action:(*)=>ShutdownSystem()} ]
}

ShowSystemMenu(*) {
    global MainMenu, MenuStack
    if (MainMenu != '') {          ; ya abierto → togglear cierra
        CloseMainMenu()
        return
    }
    MenuStack := []
    RenderMenu(WinarchySysItems(), 'System')
}

ShowMainMenu(*) {
    global MainMenu, MenuStack
    if (MainMenu != '') {          ; ya abierto → togglear cierra
        CloseMainMenu()
        return
    }
    MenuStack := []
    RenderMenu(WinarchyMenuItems(), 'Winarchy')
}

RenderMenu(items, title) {
    global MainMenu, MenuRows, MenuItems, MenuSel, MenuTitle, MenuColors, MenuSelBar
    if (MainMenu != '')
        try MainMenu.Destroy()
    MenuItems := items, MenuSel := 1, MenuRows := [], MenuTitle := title

    ; colores del theme (fallback al theme por defecto si todavía no se generó theme.ini)
    bg := '16121E', fg := 'C8C0D8', ac := '9BE53E', mut := '4A4060'
    ini := A_ScriptDir '\theme.ini'
    if FileExist(ini) {
        bg  := LTrim(IniRead(ini, 'colors', 'background', bg), '#')
        fg  := LTrim(IniRead(ini, 'colors', 'foreground', fg), '#')
        ac  := LTrim(IniRead(ini, 'colors', 'accent', ac), '#')
        mut := LTrim(IniRead(ini, 'colors', 'muted', mut), '#')
    }
    MenuColors := {bg:bg, fg:fg, ac:ac, mut:mut}

    g := Gui('-Caption +AlwaysOnTop +ToolWindow', 'WinarchyMenu')
    g.BackColor := bg
    g.MarginX := 0, g.MarginY := 0

    pad := 16, rowH := 30, titleH := 32, labelW := 250, hintW := 170, gap := 16
    w := pad * 2 + labelW + gap + hintW

    g.SetFont('s12 bold', 'JetBrainsMono Nerd Font')
    g.Add('Text', Format('x{} y{} w{} c{}', pad, pad, labelW + gap + hintW, ac), title)

    g.SetFont('s11 norm', 'JetBrainsMono Nerd Font')
    y0 := pad + titleH
    ; barra de selección accent: se agrega PRIMERO (queda detrás de los textos, que son
    ; BackgroundTrans) y se reposiciona sobre la fila activa con .Move() en SetMenuSel.
    MenuSelBar := g.Add('Text', Format('x{} y{} w{} h{} Background{}', pad - 6, y0, labelW + gap + hintW + 12, rowH, ac), '')
    for i, it in items {
        y := y0 + (i - 1) * rowH
        marker := (i = 1) ? Chr(0x25B8) ' ' : '   '
        ; fila 1 preseleccionada: texto en color fondo para leerse sobre la barra accent
        lbl := g.Add('Text', Format('x{} y{} w{} h{} c{} BackgroundTrans', pad, y + 4, labelW, rowH, (i = 1) ? bg : fg), marker it.text)
        lbl.OnEvent('Click', MenuClick.Bind(i))
        hintTxt := it.HasOwnProp('hint') ? it.hint : (it.HasOwnProp('sub') ? Chr(0x25B8) : '')
        hnt := g.Add('Text', Format('x{} y{} w{} h{} c{} Right BackgroundTrans', pad + labelW + gap, y + 4, hintW, rowH, (i = 1) ? bg : mut), hintTxt)
        hnt.OnEvent('Click', MenuClick.Bind(i))
        MenuRows.Push({label: lbl, hint: hnt, y: y})
    }
    h := y0 + items.Length * rowH + pad

    ; centrado en el monitor donde está el mouse (igual que el overlay de keybindings)
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
    g.OnEvent('Escape', (*) => MenuBack())
    g.OnEvent('Close', (*) => CloseMainMenu())
    MainMenu := g
    g.Show(Format('x{} y{} w{} h{}', wl + (wr - wl - w) // 2, wt + (wb - wt - h) // 2, w, h))
    DllCall('dwmapi\DwmSetWindowAttribute', 'ptr', g.Hwnd, 'int', 33, 'int*', 2, 'int', 4)  ; esquinas redondeadas
    SetTimer(MainMenuWatch, 250)               ; cierra al perder el foco
}

SetMenuSel(n) {
    global MenuRows, MenuItems, MenuSel, MenuColors, MenuSelBar
    n := Mod(n - 1 + MenuItems.Length, MenuItems.Length) + 1   ; wrap circular, 1-based
    ; fila vieja → colores normales
    old := MenuRows[MenuSel]
    old.label.Text := '   ' MenuItems[MenuSel].text
    old.label.SetFont('c' MenuColors.fg)
    old.hint.SetFont('c' MenuColors.mut)
    ; fila nueva → texto en color fondo (sobre la barra accent) + mover la barra
    cur := MenuRows[n]
    cur.label.Text := Chr(0x25B8) ' ' MenuItems[n].text
    cur.label.SetFont('c' MenuColors.bg)
    cur.hint.SetFont('c' MenuColors.bg)
    try MenuSelBar.Move(, cur.y)
    MenuSel := n
}

MenuNav(dir) {
    global MenuSel
    SetMenuSel(MenuSel + dir)
}

MenuActivate() {
    global MenuItems, MenuSel
    InvokeMenuItem(MenuItems[MenuSel])
}

MenuClick(i, *) {
    global MenuItems
    SetMenuSel(i)
    InvokeMenuItem(MenuItems[i])
}

InvokeMenuItem(it) {
    global MenuStack, MenuItems, MenuTitle, MenuSel
    if it.HasOwnProp('sub') {
        MenuStack.Push({items: MenuItems, title: MenuTitle, sel: MenuSel})
        RenderMenu(it.sub, it.text)
        return
    }
    CloseMainMenu()
    it.action.Call()
}

MenuBack() {
    global MenuStack
    if MenuStack.Length {
        prev := MenuStack.Pop()
        RenderMenu(prev.items, prev.title)
        SetMenuSel(prev.sel)
        return
    }
    CloseMainMenu()
}

CloseMainMenu() {
    global MainMenu, MenuStack
    SetTimer(MainMenuWatch, 0)
    try MainMenu.Destroy()
    MainMenu := '', MenuStack := []
}

MainMenuWatch() {
    global MainMenu
    if (MainMenu != '') && !WinActive('ahk_id ' MainMenu.Hwnd)
        CloseMainMenu()
}

; Navegación por teclado, solo mientras el menú está activo.
#HotIf WinActive('WinarchyMenu ahk_class AutoHotkeyGUI')
Up::MenuNav(-1)
Down::MenuNav(1)
Enter::MenuActivate()
Backspace::MenuBack()
#HotIf

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
        for s in ParseKeymap(A_ScriptDir '\user.ahk', 'User')
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

    ; NO se suspenden los hotkeys en game-mode: igual que la tecla Windows, siguen
    ; vivos durante el juego. game-mode NO pausa komorebi (solo flota la ventana del
    ; juego vía ignore-rule), así que el tiling y SUPER+1/2 siguen funcionando; los
    ; combos de geometría aplican al resto de ventanas tileadas. game-mode solo
    ; trackea estado y refresca games.toml.
    if (active && !GameModeActive) {
        GameModeActive := true
        LoadGames()                              ; refresco para altas en caliente
        Komorebic('animation disable')           ; nada de reflows animados mientras jugás
    } else if (!active && GameModeActive) {
        GameModeActive := false
        Komorebic('animation enable')
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
; HOTKEYS — esquema SUPER (siguen vivos en game-mode, como la tecla Windows)
; ============================================================================

; --- Apps ---------------------------------------------------------------------
#Enter::Run('wt.exe')                            ; terminal
#Space::ToggleFlow()                             ; Flow Launcher
#b::Run(DefaultBrowser())                        ; browser
#e::Run('explorer.exe')                          ; file explorer
; Win+N queda libre para Windows (centro de notificaciones; la campanita de YASB lo simula)
#m::LaunchMusic()                                ; music (Spotify / YT Music)
#o::LaunchObsidian()                             ; Obsidian

; --- Menu (popup estilo Omarchy) ------------------------------------------------
#!Space::ShowMainMenu()                          ; winarchy menu (Apps/Themes/System/...)
#Esc::ShowSystemMenu()                           ; system / power menu (themed)

; --- Webapps --------------------------------------------------------------------
#a::WebApp('https://chatgpt.com')                ; ChatGPT
#+a::WebApp('https://claude.ai')                 ; Claude
#y::WebApp('https://youtube.com')                ; YouTube
#x::WebApp('https://x.com')                      ; X
#c::WebApp('https://calendar.google.com')        ; Google Calendar
#g::WebApp('https://web.whatsapp.com')           ; WhatsApp (in-game Win+G = Game Bar)

; --- Windows -------------------------------------------------------------------
#w::Komorebic('close')                           ; close window
#f::Komorebic('toggle-monocle')                  ; monocle (logical fullscreen)
#+f::Komorebic('toggle-maximize')                ; real maximize
#t::Komorebic('toggle-float')                    ; float/tile
#p::Komorebic('toggle-pause')                    ; pause tiling
#r::Komorebic('retile')                          ; force retile
#+r::Winarchy('reload')                          ; reload whole stack

; --- Focus -----------------------------------------------------------
#Left::Komorebic('focus left')                   ; move focus
#Right::Komorebic('focus right')
#Up::Komorebic('focus up')
#Down::Komorebic('focus down')

; --- Move window ---------------------------------------------------
#+Left::Komorebic('move left')                   ; move window
#+Right::Komorebic('move right')
#+Up::Komorebic('move up')
#+Down::Komorebic('move down')

; --- Stacks (komorebi los dibuja como pestanas en la stackbar) --------------------
#!Left::Komorebic('stack left')                  ; apilar con la ventana vecina
#!Right::Komorebic('stack right')
#!Up::Komorebic('stack up')
#!Down::Komorebic('stack down')
#!u::Komorebic('unstack')                        ; sacar la ventana del stack
#!,::Komorebic('cycle-stack previous')           ; pestana anterior del stack
#!.::Komorebic('cycle-stack next')               ; pestana siguiente del stack

; --- Resize -----------------------------------------------------------------------
#=::Komorebic('resize-axis horizontal increase') ; width +
#-::Komorebic('resize-axis horizontal decrease') ; width -
#+=::Komorebic('resize-axis vertical increase')  ; height +
#+-::Komorebic('resize-axis vertical decrease')  ; height -

; --- Workspaces ---------------------------------------------------------------
; 1-9 (komorebi, no virtual desktops nativos)
#1::Komorebic('focus-workspace 0')               ; go to workspace N
#2::Komorebic('focus-workspace 1')
#3::Komorebic('focus-workspace 2')
#4::Komorebic('focus-workspace 3')
#5::Komorebic('focus-workspace 4')
#6::Komorebic('focus-workspace 5')
#7::Komorebic('focus-workspace 6')
#8::Komorebic('focus-workspace 7')
#9::Komorebic('focus-workspace 8')

#+1::Komorebic('move-to-workspace 0')            ; move window to workspace N
#+2::Komorebic('move-to-workspace 1')
#+3::Komorebic('move-to-workspace 2')
#+4::Komorebic('move-to-workspace 3')
#+5::Komorebic('move-to-workspace 4')
#+6::Komorebic('move-to-workspace 5')
#+7::Komorebic('move-to-workspace 6')
#+8::Komorebic('move-to-workspace 7')
#+9::Komorebic('move-to-workspace 8')

; --- Monitors ------------------------------------------------------------------------
#,::Komorebic('cycle-focus-monitor previous')    ; focus previous monitor
#.::Komorebic('cycle-focus-monitor next')        ; focus next monitor
#+,::Komorebic('cycle-move-monitor previous')    ; move to previous monitor
#+.::Komorebic('cycle-move-monitor next')        ; move to next monitor

; --- Screenshots (ShareX) -------------------------------------------------------------------
#+s::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -RectangleRegion')  ; region capture
#+w::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -ActiveWindow')     ; active window capture
#+p::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -PrintScreen')      ; fullscreen capture

; --- Themes / help --------------------------------------------------------------------
#+t::Winarchy('theme next')                       ; next theme
#+v::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -ScreenRecorder')    ; screen recording
#+g::Run('"' EnvGet('ProgramFiles') '\ShareX\ShareX.exe" -ScreenRecorderGIF') ; GIF recording
#^t::Winarchy('theme gallery')                    ; theme gallery
#^Space::Winarchy('background next')              ; next background of the active theme
#k::ToggleKeyOverlay()                            ; this keybindings overlay

; --- Override de usuario (config\ahk\user.ahk, no versionado; sus hotkeys
;     aparecen en el overlay bajo "Usuario" o sus propios headers "; --- X ---")
#HotIf
#Include *i %A_ScriptDir%\user.ahk
