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
OnError(LogAhkError)
sessionId := 0
ShellStatusFile := DllCall('ProcessIdToSessionId', 'uint', DllCall('GetCurrentProcessId'), 'uint*', &sessionId, 'int') ? StateDir "\shell-session-" sessionId : ""
ShellSessionMarker := sessionId ? StateDir "\session-shell-" sessionId : ""
AssistantPidFile := sessionId ? StateDir "\assistant-session-" sessionId : ""
ShellLogonIdentity := GetShellLogonIdentity()
GameFlag := StateDir "\game-mode.flag"
MenuFlag := StateDir "\show-menu.flag"
StartFlag := StateDir "\show-start.flag"
QuickFlag := StateDir "\show-quick-settings.flag"
NotifFlag := StateDir "\show-notifications.flag"
AwakeFlag := StateDir "\stay-awake.flag"
GamesTomls := [RepoRoot "\games.toml", RepoRoot "\games.user.toml"]
WinarchyPs1 := RepoRoot "\bin\winarchy.ps1"
DirCreate(StateDir)
WinarchyLocale := EnvGet('WINARCHY_LANG')
if (WinarchyLocale = '') {
    locale := Buffer(170, 0)
    WinarchyLocale := DllCall('GetUserDefaultLocaleName', 'Ptr', locale, 'Int', 85, 'Int') ? StrGet(locale, 'UTF-16') : 'en'
}
WinarchyLocale := StrLower(WinarchyLocale)
try FileDelete(AwakeFlag)   ; the execution state dies with this process, so a surviving flag would lie

; Asistente de voz: opt-in (`winarchy assistant on`); este script, que arranca con la sesión, lo levanta.
if FileExist(StateDir "\assistant.enabled") && !GetAssistantPid() {
    legacyPid := !FileExist(AssistantPidFile) ? FindSessionProcessPid('winarchy-assistant.exe', [RepoRoot '\state\bin\winarchy-assistant.exe', RepoRoot '\assistant\target\release\winarchy-assistant.exe'], 'serve') : 0
    if legacyPid > 0 {
        try {
            ProcessClose(legacyPid)
            ProcessWaitClose(legacyPid, 1)
        }
    }
    assistantCandidate := !GetAssistantPid() ? FindSessionProcessPid('winarchy-assistant.exe', [RepoRoot '\state\bin\winarchy-assistant.exe', RepoRoot '\assistant\target\release\winarchy-assistant.exe'], 'serve') : 0
    if (assistantCandidate = 0) {
        for exe in [StateDir "\bin\winarchy-assistant.exe", RepoRoot "\assistant\target\release\winarchy-assistant.exe"] {
            if FileExist(exe) {
                EnvSet("WINARCHY_ROOT", RepoRoot)
                Run('"' exe '" serve', , "Hide")
                break
            }
        }
    }
}

; Define WinarchyUserMenu con entradas propias del menu y del tray.
#Include *i %A_ScriptDir%\user-menu.ahk

; PID propio para que `winarchy doctor` nos detecte sin depender de WMI/CIM
try FileOpen(StateDir "\ahk.pid", "w").Write(DllCall("GetCurrentProcessId"))

; --- Watchdog de modificadores ------------------------------------------------
; Evita que Win/Alt/Ctrl queden trabados lógicamente si un cambio de foco, una
; ventana con privilegios o un hook sintético tragan el evento key-up.
InstallKeybdHook()
SetTimer(ReconcileModifiers, 200)

ReconcileModifiers() {
    toRelease := ''
    if GetKeyState('LWin') && !GetKeyState('LWin', 'P')
        toRelease .= '{LWin up}'
    if GetKeyState('RWin') && !GetKeyState('RWin', 'P')
        toRelease .= '{RWin up}'
    if GetKeyState('LAlt') && !GetKeyState('LAlt', 'P')
        toRelease .= '{LAlt up}'
    if GetKeyState('RAlt') && !GetKeyState('RAlt', 'P')
        toRelease .= '{RAlt up}'
    if GetKeyState('LCtrl') && !GetKeyState('LCtrl', 'P')
        toRelease .= '{LCtrl up}'
    if GetKeyState('RCtrl') && !GetKeyState('RCtrl', 'P')
        toRelease .= '{RCtrl up}'
    if (toRelease != '')
        Send('{Blind}{vkE8}' toRelease)   ; vkE8 enmascara: un Win/Alt-up a secas abre Inicio o la barra de menú
}

; Fallback instantáneo: presionar Escape limpia modificadores fantasma
~*Esc::{
    ReconcileModifiers()
    ShellIsland('dock.tray.dismiss')
}

; --- Config de usuario ---------------------------------------------------------
; true  = Win sola abre el menú Inicio (comportamiento nativo de Windows)
; false = Win sola es solo modificador (no abre nada)
WinAloneOpensStart := true

#HotIf !WinAloneOpensStart
~LWin::Send('{Blind}{vkE8}')    ; traga el key-up de Win sola sin afectar combos
#HotIf

; Sin Explorer no existe el menú Inicio de Windows: Win sola abre el launcher (Flow).
SessionShellActive() {
    global ShellSessionMarker, ShellLogonIdentity
    try return ShellSessionMarker && FileExist(ShellSessionMarker) && Trim(FileRead(ShellSessionMarker)) = ShellLogonIdentity
    catch
        return false
}
#HotIf SessionShellActive()
#d::ShellIsland('desktop.toggle')   ; sin Explorer nadie atiende Win+D
Volume_Up::SoundSetVolume('+2')           ; sin Explorer nadie atiende las teclas de volumen; el OSD sale solo al cambiar el nivel
Volume_Down::SoundSetVolume('-2')
Volume_Mute::SoundSetMute(-1)
#n::ShellIsland('notifications.recent')   ; sin Explorer nadie atiende Win+N: se repiten las últimas notificaciones
~LWin up:: {
    if A_PriorKey = 'LWin'
        OpenStart()
}
; Al apretar Win se suelta el confinamiento del mouse que fija un juego, para pasar el cursor a otro monitor antes de que el launcher tome el foco.
~LWin::DllCall('ClipCursor', 'ptr', 0)
; Sin Explorer no hay conmutador de Alt+Tab: un toque vuelve a la ventana anterior y Tab con Alt sostenido sigue por z-order; la shell dibuja el conmutador.
!Tab::AltTabStep(1)
!+Tab::AltTabStep(-1)
~LAlt up::AltTabStep(0)
#HotIf

AltTabStep(dir) {
    static list := [], idx := 0, shown := false
    if !dir {
        if shown
            ShellIsland('switcher.hide')
        if idx
            try WinActivate(list[idx + 1])
        shown := false
        idx := 0
        return
    }
    if !idx {
        list := []
        active := DllCall('user32\GetAncestor', 'ptr', WinExist('A'), 'uint', 3, 'ptr')
        for hwnd in WinGetList() {
            if hwnd = active || (AltTabCandidate(hwnd) && !DllCall('user32\GetWindow', 'ptr', hwnd, 'uint', 4, 'ptr'))
                list.Push(hwnd)
        }
    }
    if list.Length < 2
        return
    idx := Mod(idx + dir + list.Length, list.Length)
    ids := ''
    for hwnd in list
        ids .= (ids ? ',' : '') hwnd
    shown := ShellIsland('switcher.show', ',"params":{"windows":[' ids '],"index":' idx '}')
}

AltTabCandidate(hwnd) {
    if WinGetTitle(hwnd) = '' || (WinGetExStyle(hwnd) & 0x80) && !(WinGetExStyle(hwnd) & 0x40000)
        return false
    DllCall('dwmapi\DwmGetWindowAttribute', 'ptr', hwnd, 'uint', 14, 'int*', &cloaked := 0, 'uint', 4)
    return !cloaked && WinGetProcessName(hwnd) != 'winarchy-shell.exe'
}

; --- Helpers -----------------------------------------------------------------
; Ruta completa: no depender del PATH heredado (puede ser anterior a la instalación)
KomorebicExe := FileExist(A_ProgramFiles "\komorebi\bin\komorebic.exe")
    ? A_ProgramFiles "\komorebi\bin\komorebic.exe"
    : "komorebic.exe"

WeztermExe := FileExist(StateDir "\bin\wezterm\wezterm-gui.exe")
    ? StateDir "\bin\wezterm\wezterm-gui.exe"
    : FileExist(A_ProgramFiles "\WezTerm\wezterm-gui.exe")
    ? A_ProgramFiles "\WezTerm\wezterm-gui.exe"
    : "wezterm-gui.exe"
; --config-file explicito: WEZTERM_CONFIG_FILE puede faltar en el entorno heredado (AHK
; arrancado antes de instalar WezTerm) y ahi WezTerm cae a sus defaults, sin theme ni pwsh.
Wezterm(args := '') {
    global WeztermExe, RepoRoot
    ; El hook se traga el Enter y WinActivate puede inyectar Alt: sin otra tecla, Windows toma el Win-up como Win solo y abre Inicio.
    Send('{Blind}{vkE8}')
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

; komorebi no sigue al mouse en un escritorio vacío: sin esto SUPER+N cae en el monitor del
; último foco y no en el que se está mirando.
FocusWorkspace(index) {
    Run('"' KomorebicExe '" focus-monitor-at-cursor', , 'Hide', &pid)
    ProcessWaitClose(pid, 1)   ; acotado: un komorebi colgado no puede congelar el hotkey
    Komorebic('focus-workspace ' index)
}

; Estado vivo de komorebi como objeto JS; `first` es un comando de komorebic a correr antes.
KomorebiState(first := '') {
    global KomorebicExe
    tmp := A_Temp '\winarchy-state.json'
    RunWait(A_ComSpec ' /c "' (first ? '"' KomorebicExe '" ' first ' & ' : '') '"' KomorebicExe '" state > "' tmp '""', , 'Hide')
    doc := ComObject('htmlfile')
    doc.write('<meta http-equiv="X-UA-Compatible" content="IE=edge">')
    return doc.parentWindow.JSON.parse(FileRead(tmp))
}

; Como Hyprland `l/r/u/d`: corre `cmd <índice>` sobre el monitor más cercano de ese lado del
; enfocado; si de ese lado no hay ninguno, no hace nada.
MonitorToward(dir, cmd) {
    try {
        mons := KomorebiState().monitors
        cur := mons.elements.%mons.focused%.size   ; right/bottom son ancho/alto
        cx := cur.left + cur.right / 2, cy := cur.top + cur.bottom / 2
        best := -1, bestDist := 0
        Loop mons.elements.length {
            i := A_Index - 1
            if (i = mons.focused)
                continue
            s := mons.elements.%i%.size
            dx := s.left + s.right / 2 - cx, dy := s.top + s.bottom / 2 - cy
            along := dir = 'l' ? -dx : dir = 'r' ? dx : dir = 'u' ? -dy : dy
            across := (dir = 'l' || dir = 'r') ? Abs(dy) : Abs(dx)
            if (along <= 0 || across > along)   ; fuera del cono de 45° de esa dirección
                continue
            if (best < 0 || along < bestDist)
                best := i, bestDist := along
        }
        if (best >= 0)
            Komorebic(cmd ' ' best)
    }
}

; komorebi solo tiene un paso de resize global (50): se cambia para este resize y se restaura.
ResizeStep(args, delta) {
    global KomorebicExe
    for cmd in ['resize-delta ' delta, 'resize-axis ' args, 'resize-delta 50']
        RunWait('"' KomorebicExe '" ' cmd, , 'Hide')
}

; Como Hyprland `e+1`: salta los workspaces vacios. wrap=false (la rueda) se detiene en los extremos.
CycleOccupied(dir, wrap := true) {
    static last := 0
    if (A_TickCount - last < 120)   ; una rueda libre manda varios eventos por gesto
        return
    last := A_TickCount
    try {
        state := KomorebiState('focus-monitor-at-cursor')
        wss := state.monitors.elements.%state.monitors.focused%.workspaces
        n := wss.elements.length, cur := wss.focused
        Loop n - 1 {
            i := cur + dir * A_Index
            if (wrap)
                i := Mod(i + n, n)
            else if (i < 0 || i >= n)
                return
            w := wss.elements.%i%
            if (w.containers.elements.length || w.floating_windows.elements.length || IsObject(w.monocle_container) || IsObject(w.maximized_window)) {
                Komorebic('focus-workspace ' i)
                return
            }
        }
    } catch {
        Komorebic('cycle-workspace ' (dir > 0 ? 'next' : 'previous'))   ; sin estado legible, el ciclo plano
    }
}

; Un workspace con `tile` apagado no se reacomoda: es el floating de Windows. El ciclo va
; tiling, scrolling, floating como Omarchy, y el modo se guarda para sobrevivir un reinicio.
WorkspaceModeOf(ws) {
    if !ws.tile
        return 'floating'
    try return ws.layout.Default = 'Scrolling' ? 'scrolling' : 'tiling'
    return 'tiling'
}

CycleWorkspaceMode(dir := 1) {
    static last := 0, modes := ['tiling', 'scrolling', 'floating']
    if (A_TickCount - last < 150)
        return
    last := A_TickCount
    try {
        cur := WorkspaceModeOf(FocusedWorkspace(&m, &w))
        at := 1
        for i, name in modes {
            if (name = cur)
                at := i
        }
        SetWorkspaceMode(modes[Mod(at - 1 + dir + 3, 3) + 1], , m, w)
    }
}

; Estado del workspace enfocado del monitor bajo el cursor; m y w salen por referencia.
FocusedWorkspace(&m, &w, atCursor := true) {
    state := KomorebiState(atCursor ? 'focus-monitor-at-cursor' : '')
    m := state.monitors.focused
    wss := state.monitors.elements.%m%.workspaces
    w := wss.focused
    return wss.elements.%w%
}

; En un workspace deslizante (scrolling) izquierda/derecha recorre las columnas del propio workspace: komorebi, al llegar al borde, saltaría al monitor vecino.
; Se lee el workspace del foco real (no el del cursor) y, ante cualquier fallo, se cae al comando espacial de siempre.
HorizontalNav(verb, dir) {
    try {
        if (WorkspaceModeOf(FocusedWorkspace(&m, &w, false)) = 'scrolling') {
            Komorebic('cycle-' verb ' ' (dir = 'left' ? 'previous' : 'next'))
            return
        }
    }
    Komorebic(verb ' ' dir)
}

; layout vacío = BSP. Con m y w omitidos actúa sobre el workspace enfocado.
SetWorkspaceMode(mode, layout := '', m := '', w := '') {
    global KomorebicExe
    if (m = '')
        FocusedWorkspace(&m, &w)
    k := '"' KomorebicExe '" '
    if (mode = 'floating') {
        RunWait(k 'workspace-tiling ' m ' ' w ' disable', , 'Hide')
    } else {
        RunWait(k 'workspace-layout ' m ' ' w ' ' (layout ? layout : mode = 'scrolling' ? 'scrolling' : 'bsp'), , 'Hide')
        RunWait(k 'workspace-tiling ' m ' ' w ' enable', , 'Hide')
        RunWait(k 'retile', , 'Hide')
    }
    SetTimer(PersistWorkspaceModes, -600)   ; una ráfaga de ciclos guarda una sola vez
}

PersistWorkspaceModes() {
    Winarchy('mode save')
}

; WM_CLOSE to the active window, never to the desktop or taskbar.
CloseWindow() {
    if !(hwnd := WinExist('A'))
        return
    try {
        if WinGetClass(hwnd) ~= '^(Progman|WorkerW|Shell_TrayWnd|Shell_SecondaryTrayWnd)$'
            || WinGetProcessName(hwnd) ~= '^(yasb|winarchy-shell)\.exe$'
            return
        ; WezTerm tarda 2-4 s en cerrar (apaga los procesos del pane); oculta primero para que
        ; el tiling reaccione al instante. Seguro: window_close_confirmation = NeverPrompt.
        if WinGetProcessName(hwnd) = 'wezterm-gui.exe'
            WinHide(hwnd)
        PostMessage(0x0010, 0, 0, , hwnd)
    }
}

; Super+drag, driven by komorebi commands: AHK swallows the click, so the system
; move/size loops (and komorebi's own drag handling) never see the button held.
; Move: the window follows the cursor, then swaps with the tile it was dropped on.
; Resize: grab near an edge or corner, the drop distance becomes a resize-edge.
WindowDrag(move, held := 0) {
    global DragReleased
    ; AHK loses the Win key-up while an elevated or anti-cheat window has focus; without this
    ; every click stays swallowed as Super+drag.
    if !GetKeyState('LWin', 'P') && !GetKeyState('RWin', 'P') {
        Send('{Blind}{LWin up}{RWin up}' (move ? '{LButton down}' : '{RButton down}'))
        return
    }
    CoordMode('Mouse', 'Screen')
    MouseGetPos(&x, &y, &under)
    hwnd := DllCall('user32\GetAncestor', 'ptr', under, 'uint', 2, 'ptr')
    DragReleased := false
    t0 := A_TickCount
    held := held || (() => !DragReleased && A_TickCount - t0 < 30000)
    if !hwnd || WinGetMinMax(hwnd) = 1 || !DragTarget(hwnd) {
        while held()
            Sleep(10)
        return
    }
    try WinActivate(hwnd), WinWaitActive(hwnd, , 0.5)
    if !WinExist(hwnd) {                          ; closed between the click and the drag
        while held()
            Sleep(10)
        return
    }
    move ? DragMove(hwnd, x, y, held) : DragResize(hwnd, x, y, held)
}

; AHK swallows the click, so the release is read from pass-through hotkeys instead of key state.
DragReleased := true
*~LButton Up::DragRelease()
*~RButton Up::DragRelease()
DragRelease() {
    global DragReleased := true
}

; A real, visible app window: not the desktop, taskbar, bar or a tool window.
DragTarget(hwnd) {
    return (WinGetStyle(hwnd) & 0x10000000) && !(WinGetExStyle(hwnd) & 0x80)
        && WinGetMinMax(hwnd) != -1
        && !(WinGetClass(hwnd) ~= '^(Progman|WorkerW|Shell_TrayWnd|Shell_SecondaryTrayWnd)$')
        && !(WinGetProcessName(hwnd) ~= '^(yasb|winarchy-shell)\.exe$')
}

; Floating workspace (tile off) or a floating window: there is no tile to swap with or snap back
; to, so the window keeps where it was dropped. Unreadable state falls back to the tiled path.
DragIsFree(hwnd) {
    try {
        ws := FocusedWorkspace(&m, &w)
        if !ws.tile
            return true
        floats := ws.floating_windows.elements
        Loop floats.length {
            if (floats.%A_Index - 1%.hwnd = hwnd)
                return true
        }
    }
    return false
}

DragMove(hwnd, x0, y0, held) {
    WinGetPos(&wx, &wy, &ww, &wh, hwnd)
    SetWinDelay(-1)
    DllCall('winmm\timeBeginPeriod', 'uint', 1)    ; Sleep(1) otherwise rounds up to ~15 ms
    px := x0, py := y0
    while held() {
        MouseGetPos(&x, &y)
        if (x != px || y != py) {
            px := x, py := y
            DllCall('user32\SetWindowPos', 'ptr', hwnd, 'ptr', 0, 'int', wx + x - x0, 'int', wy + y - y0, 'int', 0, 'int', 0, 'uint', 0x15)   ; NOSIZE | NOZORDER | NOACTIVATE
        }
        Sleep(1)
    }
    DllCall('winmm\timeEndPeriod', 'uint', 1)
    if DragIsFree(hwnd)
        return
    DllCall('user32\SetWindowPos', 'ptr', hwnd, 'ptr', 0, 'int', wx, 'int', wy, 'int', 0, 'int', 0, 'uint', 0x15)
    MouseGetPos(&x, &y, &under)                  ; what is visible under the cursor once the window is back
    t := DllCall('user32\GetAncestor', 'ptr', under, 'uint', 2, 'ptr')
    while t && WinGetProcessName(t) = 'komorebi.exe' {   ; focus borders overhang the tile; look through them
        loop {
            t := DllCall('user32\GetWindow', 'ptr', t, 'uint', 2, 'ptr')   ; GW_HWNDNEXT
            if !t
                break
            WinGetPos(&tx, &ty, &tw, &th, t)
            if (WinGetStyle(t) & 0x10000000) && x >= tx && x < tx + tw && y >= ty && y < ty + th
                break
        }
    }
    if !t || t = hwnd || !DragTarget(t)
        return                                   ; dropped on itself: stay put
    WinGetPos(&tx, &ty, &tw, &th, t)
    dx := tx + tw // 2 - (wx + ww // 2), dy := ty + th // 2 - (wy + wh // 2)
    Komorebic('move ' (Abs(dx) >= Abs(dy) ? (dx < 0 ? 'left' : 'right') : (dy < 0 ? 'up' : 'down')))
}

DragResize(hwnd, x0, y0, held) {
    WinGetPos(&wx, &wy, &ww, &wh, hwnd)
    col := Min(Max((x0 - wx) * 3 // ww, 0), 2), row := Min(Max((y0 - wy) * 3 // wh, 0), 2)
    skipX := col = 1 && row != 1, skipY := row = 1 && col != 1
    DllCall('winmm\timeBeginPeriod', 'uint', 1)
    px := x0, py := y0
    while held() {                                  ; live preview: the window follows the grabbed edge
        MouseGetPos(&x, &y)
        if (x != px || y != py) {
            px := x, py := y
            dx := skipX ? 0 : x - x0, dy := skipY ? 0 : y - y0
            nx := col = 0 ? wx + dx : wx, nw := Max(col = 0 ? ww - dx : ww + dx, 200)
            ny := row = 0 ? wy + dy : wy, nh := Max(row = 0 ? wh - dy : wh + dy, 150)
            DllCall('user32\SetWindowPos', 'ptr', hwnd, 'ptr', 0, 'int', nx, 'int', ny, 'int', nw, 'int', nh, 'uint', 0x14)   ; NOZORDER | NOACTIVATE
        }
        Sleep(1)
    }
    DllCall('winmm\timeEndPeriod', 'uint', 1)
    if DragIsFree(hwnd)
        return
    MouseGetPos(&x, &y)
    dx := x - x0, dy := y - y0
    K := (cmd) => RunWait('"' KomorebicExe '" ' cmd, , 'Hide')
    committed := false
    for axis in [[dx, col = 0 ? 'left' : 'right', skipX], [dy, row = 0 ? 'up' : 'down', skipY]] {
        d := axis[1], edge := axis[2]
        if axis[3] || Abs(d) < 20
            continue
        committed := true
        grows := (edge = 'left' || edge = 'up') ? d < 0 : d > 0
        K('resize-delta ' Abs(d) * 2)               ; komorebi moves the shared edge by half the delta
        K('resize-edge ' edge (grows ? ' increase' : ' decrease'))
    }
    K('resize-delta 50')
    if !committed
        DllCall('user32\SetWindowPos', 'ptr', hwnd, 'ptr', 0, 'int', wx, 'int', wy, 'int', ww, 'int', wh, 'uint', 0x14)
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

WLog(text) {
    path := StateDir '\winarchy.log'
    try if FileGetSize(path) > 524288
        FileMove(path, path '.1', true)
    try FileAppend(FormatTime(, 'yyyy-MM-dd HH:mm:ss') '.' A_MSec ' ahk ' text '`n', path, 'UTF-8-RAW')
}

; devuelve 0: queda además el aviso normal de AutoHotkey
LogAhkError(err, mode) {
    try WLog('ERROR ' (IsObject(err) ? err.Message ' ' err.Extra ' @ ' err.File ':' err.Line : String(err)))
    return 0
}

; Dónde quedó el lanzador y dónde estaba el mouse: para diagnosticar una ventana fuera de la vista.
LogFlowPlacement(hwnd) {
    try {
        WinGetPos(&x, &y, &w, &h, 'ahk_id ' hwnd)
        CoordMode('Mouse', 'Screen')
        MouseGetPos(&mx, &my)
        WLog('flow shown at ' x ',' y ' ' w 'x' h ' mouse ' mx ',' my ' minmax ' WinGetMinMax('ahk_id ' hwnd))
    }
}

ToggleFlow() {
    ; Reusa el hotkey nativo de Flow (Alt+Space) para mostrar/ocultar su ventana ya
    ; existente, en vez de relanzar el exe.
    global FlowWindow
    flow := EnvGet('LOCALAPPDATA') '\FlowLauncher\Flow.Launcher.exe'
    if !FileExist(flow)
        return 0
    if ProcessExist('Flow.Launcher.exe') {
        wasVisible := WinExist(FlowWindow)
        Send('!{Space}')   ; sin {Blind}: un Win-up crudo con Win apretada abre el menú Inicio
        if wasVisible
            return 0
        hwnd := WinWait(FlowWindow, , 2)
        if hwnd
            SetTimer(LogFlowPlacement.Bind(hwnd), -300)
        else
            WLog('flow did not show after Alt+Space')
        return hwnd ? hwnd : 0
    }
    ; Cold start: todavía no hay proceso corriendo (recién logueado), así que tampoco
    ; hay hotkey de Flow registrado — acá sí hace falta lanzar el exe.
    Run('"' flow '"')
    ; Tras el logon Flow sigue indexando programas y tarda bastante más que los 2 s que
    ; esperábamos antes: se vencía el WinWait, no se activaba nada y la primera pulsación
    ; del día se sentía muerta.
    hwnd := WinWait(FlowWindow, , 5)
    if !hwnd {
        WLog('flow did not show after cold start')
        return 0
    }
    SetTimer(LogFlowPlacement.Bind(hwnd), -300)
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

; terminals keep Ctrl+C / Ctrl+V for the shell: their clipboard is on Ctrl+Shift
InTerminal() {
    return WinActive('ahk_exe wezterm-gui.exe') || WinActive('ahk_exe WindowsTerminal.exe')
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
    ; rundll32 no pasa los tres argumentos y con hibernación activa terminaba hibernando
    DllCall('powrprof\SetSuspendState', 'char', 0, 'char', 0, 'char', 0)
}

HibernateSystem() {
    Run('shutdown.exe /h', , 'Hide')
}

SwitchUserSystem() {
    ; desconecta la sesión sin cerrarla: la pantalla de inicio de sesión permite entrar con otra cuenta
    DllCall('wtsapi32\WTSDisconnectSession', 'ptr', 0, 'uint', 0xFFFFFFFF, 'int', 0)
}

ProjectDisplay() {
    Run('DisplaySwitch.exe')
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
    global MenuFlag, StartFlag, QuickFlag, NotifFlag
    ; Win+A y Win+N nativos: lo que manda AHK no dispara los hotkeys propios (#a es Ctrl+A) y abre y cierra el panel como la taskbar.
    if FileExist(QuickFlag) {
        try FileDelete(QuickFlag)
        catch
            return
        Send('#a')
    }
    if FileExist(NotifFlag) {
        try FileDelete(NotifFlag)
        catch
            return
        Send('#n')
    }
    if FileExist(StartFlag) {
        try FileDelete(StartFlag)
        catch
            return
        if SessionShellActive()
            ToggleFlow()
        else
            Send('{LWin}')
    }
    if FileExist(MenuFlag) {
        try FileDelete(MenuFlag)
        catch
            return   ; flag aún tomado por YASB: sin borrarlo, cada tick alternaría abrir/cerrar el menú
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

ShowTaskbars() {
    ; una shell matada a la fuerza no devuelve la taskbar que ocultó
    prev := A_DetectHiddenWindows
    DetectHiddenWindows(true)
    for class in ['Shell_TrayWnd', 'Shell_SecondaryTrayWnd']
        for hwnd in WinGetList('ahk_class ' class)
            DllCall('ShowWindow', 'Ptr', hwnd, 'Int', 8)
    DetectHiddenWindows(prev)
}

RunShellRecoveryBridge(timeoutMs := 60000) {
    global RepoRoot, ShellRecoveryBridgeHandle, ShellRecoveryBridgeBusy
    if ShellRecoveryBridgeBusy
        return false
    ShellRecoveryBridgeBusy := true
    try {
        if ShellRecoveryBridgeHandle {
            state := DllCall('WaitForSingleObject', 'ptr', ShellRecoveryBridgeHandle, 'uint', 0, 'uint')
            if state = 0 || state != 0x102 {
                DllCall('CloseHandle', 'ptr', ShellRecoveryBridgeHandle)
                ShellRecoveryBridgeHandle := 0
            }
            return false
        }

        bridge := RepoRoot '\\scripts\\Stop-WinarchySessionShell.ps1'
        command := 'pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' bridge '" -Root "' RepoRoot '"'
        startup := Buffer(A_PtrSize = 8 ? 104 : 68, 0)
        NumPut('uint', startup.Size, startup)
        processInfo := Buffer(A_PtrSize * 2 + 8, 0)
        commandBuffer := Buffer((StrLen(command) + 1) * 2, 0)
        StrPut(command, commandBuffer, 'UTF-16')
        if !DllCall('CreateProcessW', 'ptr', 0, 'ptr', commandBuffer.Ptr, 'ptr', 0, 'ptr', 0, 'int', false, 'uint', 0x08000000, 'ptr', 0, 'ptr', 0, 'ptr', startup.Ptr, 'ptr', processInfo.Ptr, 'int')
            return false

        handle := NumGet(processInfo, 0, 'ptr')
        DllCall('CloseHandle', 'ptr', NumGet(processInfo, A_PtrSize, 'ptr'))
        ShellRecoveryBridgeHandle := handle
        started := DllCall('GetTickCount64', 'uint64')
        loop {
            state := DllCall('WaitForSingleObject', 'ptr', handle, 'uint', 0, 'uint')
            if state = 0 {
                exitCode := 1
                succeeded := DllCall('GetExitCodeProcess', 'ptr', handle, 'uint*', &exitCode, 'int')
                DllCall('CloseHandle', 'ptr', handle)
                ShellRecoveryBridgeHandle := 0
                return succeeded && exitCode = 0
            }
            if state != 0x102 {
                DllCall('CloseHandle', 'ptr', handle)
                ShellRecoveryBridgeHandle := 0
                return false
            }
            if DllCall('GetTickCount64', 'uint64') - started >= timeoutMs
                return false
            Sleep(25)
        }
    } finally {
        ShellRecoveryBridgeBusy := false
    }
}

QuitStack() {
    global ShellSessionMarker, ShellStatusFile, ShellLogonIdentity, ShellLifecycleStopping, ShellWatchBusy, RepoRoot
    sessionShell := false
    statusValid := false
    if ShellStatusFile && FileExist(ShellStatusFile) {
        try status := StrSplit(Trim(FileRead(ShellStatusFile)), '|')
        catch
            status := []
        if status.Length = 3 && RegExMatch(status[1], '^\d+$') && status[3] = ShellLogonIdentity && status[2] ~= '^[01]$' {
            statusValid := true
            sessionShell := status[2] = '1'
        }
    }
    if !statusValid && ShellSessionMarker && FileExist(ShellSessionMarker) {
        try sessionShell := Trim(FileRead(ShellSessionMarker)) = ShellLogonIdentity
        catch
            sessionShell := false
    }
    shellPid := GetShellPid()
    if sessionShell {
        if ShellLifecycleStopping
            return
        ShellLifecycleStopping := true
        ShellWatchBusy := true
        SetTimer(ShellWatch, 0)
        result := RunShellRecoveryBridge()
        ShellWatchBusy := false
        if !result {
            ShellLifecycleStopping := false
            SetTimer(ShellWatch, 5000)
            TrayTip('Explorer recovery did not finish; Winarchy services are still running.', 'Winarchy')
            return
        }
    }
    ; parada ordenada y NO elevada: el shell personalizado entrega el escritorio antes del teardown
    SetTimer(YasbWatch, 0)
    SetTimer(ShellWatch, 0)
    try FileOpen(StateDir '\komorebi-stop', 'w').Close()   ; la shell no lo trata como muerte
    try Komorebic('stop')
    try RunWait('taskkill /IM yasb.exe /F', , 'Hide')
    if !sessionShell {
        ShellIsland('quit')
        if shellPid && ProcessWaitClose(shellPid, 1.5) && GetShellPid() = shellPid {
            try ProcessClose(shellPid)
            ShowTaskbars()
        }
    }
    try RunWait('taskkill /IM Flow.Launcher.exe /F', , 'Hide')
    ExitApp()
}

; --- YASB watchdog --------------------------------------------------------------
; Relaunches YASB after two missed checks; paused while winget runs; gives up after 3 relaunches in 5 min.
YasbcExe := FileExist(A_ProgramFiles "\YASB\yasbc.exe") ? A_ProgramFiles "\YASB\yasbc.exe" : ""
YasbMisses := 0
YasbRelaunches := []
YasbCooldown := 0
BarFile := StateDir "\bar"
if YasbcExe
    SetTimer(YasbWatch, 5000)

YasbWatch() {
    global YasbMisses, YasbRelaunches, YasbCooldown, BarFile
    if FileExist(BarFile) && Trim(FileRead(BarFile)) = 'native'
        return
    if A_TickCount < YasbCooldown
        return
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
        YasbCooldown := A_TickCount + 600000   ; pausa 10 min y reintenta: una caída pasajera no deja la barra muerta hasta reiniciar AHK
        TrayTip('The bar keeps crashing; pausing relaunches for 10 min. Run: winarchy doctor', 'Winarchy')
        return
    }
    YasbRelaunches.Push(A_TickCount)
    EnvSet('YASB_CONFIG_HOME', RepoRoot '\config\yasb')
    Run('"' YasbcExe '" start', , 'Hide')
}
GetShellLogonIdentity() {
    token := 0
    if !DllCall('advapi32\OpenProcessToken', 'ptr', DllCall('GetCurrentProcess', 'ptr'), 'uint', 8, 'ptr*', &token, 'int')
        throw OSError()
    stats := Buffer(56, 0), returned := 0
    ok := DllCall('advapi32\GetTokenInformation', 'ptr', token, 'uint', 10, 'ptr', stats, 'uint', stats.Size, 'uint*', &returned, 'int')
    DllCall('CloseHandle', 'ptr', token)
    if !ok || returned < stats.Size
        throw OSError()
    luid := Buffer(8, 0)
    low := NumGet(stats, 8, 'uint'), high := NumGet(stats, 12, 'uint')
    NumPut('uint', low, luid, 0), NumPut('uint', high, luid, 4)
    data := 0
    if DllCall('secur32\LsaGetLogonSessionData', 'ptr', luid, 'ptr*', &data, 'int') || !data
        throw Error('Could not read the current Windows logon identity.')
    logonTime := NumGet(data, A_PtrSize = 8 ? 80 : 60, 'int64')
    DllCall('secur32\LsaFreeReturnBuffer', 'ptr', data, 'int')
    if logonTime <= 0
        throw Error('The current Windows logon identity is invalid.')
    return Format('{:08x}{:08x}-{:016x}', high, low, logonTime)
}


; --- Native shell watchdog ----------------------------------------------------------
; Same policy as YasbWatch: two missed checks (a reload restarts it within seconds), at most 3 relaunches in 5 min.
; A check is "missing" (no process) or "hung" (a visible window of it does not answer WM_NULL within 2 s).
ShellMisses := 0
ShellWatchBusy := false
ShellLifecycleStopping := false
ShellRecoveryAttempted := false
ShellRecoveryBridgeHandle := 0
ShellRecoveryBridgeBusy := false
ShellRelaunches := []
ShellBootUntil := 0
SetTimer(ShellWatch, 5000)
; Sin Explorer nadie ejecuta Run/RunOnce ni Startup: la shell de sesión lo delega acá, una vez por inicio de sesión.
SessionAutostartTries := 0
SetTimer(SessionAutostart, 5000)
SessionAutostart() {
    global ShellSessionMarker, ShellLogonIdentity, RepoRoot, SessionAutostartTries
    if ++SessionAutostartTries > 12 {
        SetTimer(SessionAutostart, 0)
        return
    }
    try active := ShellSessionMarker && FileExist(ShellSessionMarker) && Trim(FileRead(ShellSessionMarker)) = ShellLogonIdentity
    catch
        return
    if !active
        return
    SetTimer(SessionAutostart, 0)
    Run('powershell.exe -NoProfile -ExecutionPolicy Bypass -File "' RepoRoot '\scripts\Start-SessionAutostart.ps1" -Identity "' ShellLogonIdentity '"', , 'Hide')
}
AcquireShellLifecycleMutex() {
    global sessionId
    handle := DllCall('CreateMutexW', 'ptr', 0, 'int', 0, 'str', 'Local\WinarchyShellLifecycle-' sessionId, 'ptr')
    if !handle
        return 0
    wait := DllCall('WaitForSingleObject', 'ptr', handle, 'uint', 0, 'uint')
    if wait != 0 && wait != 0x80 {
        DllCall('CloseHandle', 'ptr', handle)
        return 0
    }
    return handle
}

ReleaseShellLifecycleMutex(handle) {
    if handle {
        DllCall('ReleaseMutex', 'ptr', handle)
        DllCall('CloseHandle', 'ptr', handle)
    }
}

ShellRelaunchInhibited() {
    global StateDir, sessionId, ShellLogonIdentity
    path := StateDir '\shell-relaunch-inhibit-' sessionId
    if !FileExist(path)
        return false
    try return Trim(FileRead(path)) = ShellLogonIdentity
    catch
        return true
}


ReapShellRecoveryBridge() {
    global ShellRecoveryBridgeHandle, ShellRecoveryBridgeBusy
    if !ShellRecoveryBridgeHandle || ShellRecoveryBridgeBusy
        return
    state := DllCall('WaitForSingleObject', 'ptr', ShellRecoveryBridgeHandle, 'uint', 0, 'uint')
    if state != 0x102 {
        DllCall('CloseHandle', 'ptr', ShellRecoveryBridgeHandle)
        ShellRecoveryBridgeHandle := 0
    }
}

ShellWatch() {
    global ShellMisses, ShellRelaunches, ShellBootUntil, ShellStatusFile, ShellSessionMarker, ShellLogonIdentity, sessionId, BarFile, YasbcExe, RepoRoot, ShellWatchBusy, ShellLifecycleStopping, ShellRecoveryAttempted
    ReapShellRecoveryBridge()
    if ShellLifecycleStopping
        return
    try native := FileExist(BarFile) ? Trim(FileRead(BarFile)) = 'native' : !YasbcExe
    catch
        return   ; state\bar is being atomically replaced; check again in 5 s
    if !native {
        ShellMisses := 0
        return
    }
    if ShellRelaunchInhibited() {
        ShellMisses := 0
        ShellBootUntil := 0
        return
    }
    sessionShell := false
    if ShellSessionMarker && FileExist(ShellSessionMarker) {
        try sessionShell := Trim(FileRead(ShellSessionMarker)) = ShellLogonIdentity
        catch
            sessionShell := false
    }
    hadShellState := sessionShell || ShellStatusFile && FileExist(ShellStatusFile)
    pid := 0, statusValid := false, reportedSessionShell := false
    if ShellStatusFile && FileExist(ShellStatusFile) {
        try status := StrSplit(Trim(FileRead(ShellStatusFile)), '|')
        catch
            status := []
        if status.Length = 3 && RegExMatch(status[1], '^\d+$') && status[3] = ShellLogonIdentity && status[2] ~= '^[01]$' {
            statusValid := true
            reportedSessionShell := status[2] = '1'
            try candidate := Integer(status[1])
            catch
                candidate := 0
            candidateSession := 0
            if candidate >= 1 && candidate <= 4294967295 && ProcessExist(candidate) && DllCall('ProcessIdToSessionId', 'uint', candidate, 'uint*', &candidateSession, 'int') && candidateSession = sessionId {
                try if StrLower(ProcessGetName(candidate)) = 'winarchy-shell.exe'
                    pid := candidate
            }
        }
    } else if pid := GetShellPid() {
        ShellMisses := 0
        return
    }
    modeMismatch := sessionShell ? (!statusValid || !reportedSessionShell) : statusValid && reportedSessionShell != sessionShell
    static watchedPid := 0
    if pid != watchedPid {
        watchedPid := pid
        ShellMisses := 0
    }
    if pid {
        if ShellBootUntil = 0
            ShellBootUntil := A_TickCount + 20000
        if !modeMismatch && ShellResponsive(pid) {
            ShellMisses := 0
            ShellBootUntil := 0
            ShellRecoveryAttempted := false
            return
        }
        if !modeMismatch && !WinGetList('ahk_class WinarchyShellPill ahk_pid ' pid).Length && A_TickCount < ShellBootUntil
            return
    }
    ; la shell de sesión colgada se aborta sola en ~40 s y Windows la relanza; Explorer es solo el respaldo
    limit := pid && reportedSessionShell ? 12 : 2
    if (++ShellMisses < limit) || ShellWatchBusy
        return
    ShellWatchBusy := true
    handle := DllCall('CreateMutexW', 'ptr', 0, 'int', 0, 'str', 'Local\WinarchyShellLifecycle-' sessionId, 'ptr')
    if !handle {
        ShellWatchBusy := false
        return
    }
    wait := DllCall('WaitForSingleObject', 'ptr', handle, 'uint', 0, 'uint')
    if wait != 0 && wait != 0x80 {
        DllCall('CloseHandle', 'ptr', handle)
        ShellWatchBusy := false
        return
    }
    recoverSessionShell := false
    try {
        if ShellRelaunchInhibited() {
            ShellMisses := 0
            ShellBootUntil := 0
            return
        }
        sessionShell := false
        if ShellSessionMarker && FileExist(ShellSessionMarker) {
            try sessionShell := Trim(FileRead(ShellSessionMarker)) = ShellLogonIdentity
            catch
                sessionShell := false
        }
        statusValid := false, reportedSessionShell := false
        if ShellStatusFile && FileExist(ShellStatusFile) {
            try status := StrSplit(Trim(FileRead(ShellStatusFile)), '|')
            catch
                status := []
            if status.Length = 3 && RegExMatch(status[1], '^\d+$') && status[3] = ShellLogonIdentity && status[2] ~= '^[01]$' {
                statusValid := true
                reportedSessionShell := status[2] = '1'
            }
        }
        if statusValid && reportedSessionShell
            sessionShell := true
        modeMismatch := sessionShell ? (!statusValid || !reportedSessionShell) : statusValid && reportedSessionShell != sessionShell
        pid := GetShellPid()
        if pid && !modeMismatch && ShellResponsive(pid) {
            ShellMisses := 0
            ShellBootUntil := 0
            ShellRecoveryAttempted := false
            return
        }
        hadShellState := sessionShell || ShellStatusFile && FileExist(ShellStatusFile)
        ShellMisses := 0
        ShellBootUntil := 0
        exe := ''
        for p in [RepoRoot '\state\bin\winarchy-shell.exe', RepoRoot '\shell\target\release\winarchy-shell.exe']
            if FileExist(p) {
                exe := p
                break
            }
        while ShellRelaunches.Length && A_TickCount - ShellRelaunches[1] > 300000
            ShellRelaunches.RemoveAt(1)
        ; una shell de sesión viva pero sin responder no se cierra ni se reemplaza; solo una muerta se relanza
        if sessionShell && (pid || !exe || ShellRelaunches.Length >= 3) {
            if !ShellRecoveryAttempted
                recoverSessionShell := true
        } else if !exe || ShellRelaunches.Length >= 3 {
            static pauseTip := -300000
            if ShellRelaunches.Length >= 3 && A_TickCount - pauseTip >= 300000 {
                pauseTip := A_TickCount
                WLog('shell watch: relaunch paused, the shell keeps crashing')
                TrayTip('The shell keeps crashing; relaunch paused. Run: winarchy doctor', 'Winarchy')
            }
        } else {
            if pid {
                try ProcessClose(pid)
                if ProcessExist(pid) {
                    if hadShellState
                        ShowTaskbars()
                    WLog('shell watch: hung shell pid ' pid ' could not be stopped')
                    TrayTip('The shell is hung and could not be stopped; relaunch paused. Run: winarchy doctor', 'Winarchy')
                    return
                }
            }
            if !recoverSessionShell {
                if hadShellState && !sessionShell
                    ShowTaskbars()
                ShellRelaunches.Push(A_TickCount)
                WLog('shell watch: relaunch ' ShellRelaunches.Length ' in 5 min (previous pid ' pid ', session shell ' sessionShell ')')
                Run('"' exe '" --root "' RepoRoot '"' (sessionShell ? ' --session-shell' : ''), , 'Hide')
            }
        }
    } finally {
        DllCall('ReleaseMutex', 'ptr', handle)
        DllCall('CloseHandle', 'ptr', handle)
        ShellWatchBusy := recoverSessionShell
    }
    if recoverSessionShell {
        WLog('shell watch: session shell unresponsive, restoring Explorer')
        ShellRecoveryAttempted := true
        ShellLifecycleStopping := true
        ShellWatchBusy := true
        SetTimer(ShellWatch, 0)
        result := RunShellRecoveryBridge()
        ShellLifecycleStopping := false
        ShellWatchBusy := false
        SetTimer(ShellWatch, 5000)
        if !result {
            SetTimer(RetryShellRecovery, -300000)
            TrayTip('Automatic Explorer recovery did not finish; retrying in 5 minutes.', 'Winarchy')
        }
    }
}

RetryShellRecovery() {
    global ShellRecoveryAttempted
    ShellRecoveryAttempted := false
}

FindSessionProcessPid(exeName, paths, commandArg := '') {
    global sessionId
    capacity := 1024
    loop {
        pids := Buffer(capacity * 4, 0), bytes := 0
        if !DllCall('Psapi.dll\EnumProcesses', 'ptr', pids, 'uint', pids.Size, 'uint*', &bytes, 'int')
            return 0
        if bytes < pids.Size
            break
        if capacity >= 262144
            return 0
        capacity *= 2
    }
    ; ProcessExist only returns one match and may select another RDP session.
    Loop (bytes // 4) {
        pid := NumGet(pids, (A_Index - 1) * 4, 'uint')
        candidateSession := 0
        if pid < 1 || !DllCall('ProcessIdToSessionId', 'uint', pid, 'uint*', &candidateSession, 'int') || candidateSession != sessionId
            continue
        try {
            if StrLower(ProcessGetName(pid)) != exeName
                continue
            path := StrLower(ProcessGetPath(pid))
            matchesPath := false
            for candidatePath in paths
                if path = StrLower(candidatePath) {
                    matchesPath := true
                    break
                }
            if !matchesPath
                continue
            if commandArg {
                try {
                    commandLine := ''
                    for process in ComObjGet('winmgmts:').ExecQuery('SELECT CommandLine FROM Win32_Process WHERE ProcessId = ' pid)
                        commandLine := StrLower(process.CommandLine)
                } catch {
                    return -1
                }
                if (commandLine = '')
                    return -1
                commandKind := GetAssistantCommandKind(path, commandLine)
                if (commandKind = 'unknown')
                    return -1
                if (commandKind != commandArg)
                    continue
            }
            return pid
        } catch {
            continue
        }
    }
    return 0
}

GetAssistantCommandKind(path, commandLine) {
    commandLine := StrLower(commandLine)
    if (commandLine = '')
        return 'unknown'
    quotedPath := '"' path '"'
    if SubStr(commandLine, 1, StrLen(quotedPath)) = quotedPath
        offset := StrLen(quotedPath)
    else if SubStr(commandLine, 1, StrLen(path)) = path
        offset := StrLen(path)
    else
        return 'unknown'
    if (StrLen(commandLine) > offset && !InStr(' `t', SubStr(commandLine, offset + 1, 1)))
        return 'unknown'
    if !RegExMatch(Trim(SubStr(commandLine, offset + 1)), '^([^\s]+)', &match)
        return 'unknown'
    argument := match[1]
    if (argument = 'serve')
        return 'serve'
    if (argument = 'ask' || argument = 'login' || argument = 'logout' || argument = 'status' || argument = 'levels' || argument = 'listen')
        return 'cli'
    return 'unknown'
}
GetShellPid() {
    global ShellStatusFile, ShellLogonIdentity, sessionId, RepoRoot
    if !ShellStatusFile || !FileExist(ShellStatusFile)
        return FindSessionProcessPid('winarchy-shell.exe', [RepoRoot '\state\bin\winarchy-shell.exe', RepoRoot '\shell\target\release\winarchy-shell.exe'])
    try status := StrSplit(Trim(FileRead(ShellStatusFile)), '|')
    catch
        return 0
    if status.Length != 3 || !RegExMatch(status[1], '^\d+$') || !(status[2] ~= '^[01]$') || status[3] != ShellLogonIdentity
        return 0
    try pid := Integer(status[1])
    catch
        return 0
    candidateSession := 0
    if pid < 1 || pid > 4294967295 || !ProcessExist(pid) || !DllCall('ProcessIdToSessionId', 'uint', pid, 'uint*', &candidateSession, 'int') || candidateSession != sessionId
        return 0
    try if StrLower(ProcessGetName(pid)) = 'winarchy-shell.exe'
        return pid
    return 0
}

ShellResponsive(pid) {
    prev := A_DetectHiddenWindows
    DetectHiddenWindows(true)
    try {
        for hwnd in WinGetList('ahk_class WinarchyShellPill ahk_pid ' pid) {
            result := 0
            if DllCall('user32\SendMessageTimeoutW', 'ptr', hwnd, 'uint', 0, 'uptr', 0, 'ptr', 0, 'uint', 2, 'uint', 2000, 'ptr*', &result, 'uptr')
                return true
        }
    } finally {
        DetectHiddenWindows(prev)
    }
    return false
}

; --- Monitor watchdog -------------------------------------------------------------
; komorebi can lose a monitor when a display powers off and on, and only a restart brings it
; back. No polling: WM_DISPLAYCHANGE, checked once komorebi had time to reconcile the burst.
OnMessage(0x7E, (*) => SetTimer(HealMonitors, -20000))
OnMessage(0x8000 + 21, (*) => CycleWorkspaceMode(1))   ; clic en el glifo de modo de la Isla

HealMonitors() {
    global GameModeActive, GameFlag, WinarchyPs1
    if GameModeActive || FileExist(GameFlag) {   ; restarting the tiler under a running game steals its focus
        SetTimer(HealMonitors, -60000)
        return
    }
    code := RunWait('pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' WinarchyPs1 '" heal-monitors', , 'Hide')
    if (code = 3)
        TrayTip('komorebi lost track of a monitor and was restarted; windows were regrouped.', 'Winarchy')
    else if (code != 0)
        TrayTip('komorebi lost track of a monitor and could not be restarted. Run: winarchy doctor', 'Winarchy')
}

; --- Menu items (palette menu and tray share them) ---------------------------------
WinarchyText(text) {
    global WinarchyLocale
    static es := Map(
        'System tray', 'Bandeja del sistema', 'Apps', 'Aplicaciones', 'Themes', 'Temas', 'Appearance', 'Apariencia',
        'Capture', 'Capturas', 'Tiling', 'Mosaico', 'Bar', 'Barra', 'Coding agent', 'Agente de código',
        'Keybindings', 'Atajos de teclado', 'Reload stack', 'Recargar entorno', 'Game mode', 'Modo de juego',
        'Stay awake', 'Mantener activo', 'On', 'Activado', 'Off', 'Desactivado', 'Doctor', 'Diagnóstico',
        'Check for updates', 'Buscar actualizaciones', 'System', 'Sistema', 'Quit Winarchy', 'Cerrar Winarchy',
        'Next theme', 'Siguiente tema', 'Theme gallery', 'Galería de temas', 'Wallpaper gallery', 'Galería de fondos', 'Next background', 'Siguiente fondo',
        'Layout', 'Distribución', 'Manage this window', 'Administrar esta ventana',
        'Unmanage this window', 'Dejar de administrar esta ventana', 'Stop tiling this workspace', 'Desactivar mosaico en este escritorio',
        'New windows: stack / tile', 'Nuevas ventanas: apilar / mosaico', 'Title bars', 'Barras de título',
        'Mouse follows focus', 'El mouse sigue el foco', 'Restore hidden windows', 'Restaurar ventanas ocultas',
        'BSP', 'BSP', 'split in halves', 'dividir en mitades', 'Grid', 'Cuadrícula', 'even 2×2 grid', 'cuadrícula 2 × 2',
        'Columns', 'Columnas', 'side by side', 'lado a lado', 'Rows', 'Filas', 'stacked rows', 'filas apiladas',
        'Vertical stack', 'Pila vertical', 'main left', 'principal a la izquierda',
        'Right main stack', 'Pila con principal derecha', 'main right', 'principal a la derecha',
        'Horizontal stack', 'Pila horizontal', 'main on top', 'principal arriba',
        'Ultrawide stack', 'Pila ultrapanorámica', 'main in centre', 'principal al centro',
        'Scrolling', 'Desplazamiento', 'scrolling strip', 'tira desplazable', 'Next mode', 'Modo siguiente', 'Floating', 'Flotante', 'windows stay where you put them', 'Las ventanas quedan donde las pongas',
        'Move to top', 'Mover arriba', 'Move to bottom', 'Mover abajo', 'Toggle transparency', 'Alternar transparencia',
        'Lock', 'Bloquear', 'Screensaver', 'Protector de pantalla', 'Sleep', 'Suspender', 'Hibernate', 'Hibernar',
        'Switch user', 'Cambiar de usuario', 'Project', 'Proyectar', 'Sign out', 'Cerrar sesión', 'Restart', 'Reiniciar', 'Shut down', 'Apagar', 'Settings', 'Configuración',
        'Region', 'Región', 'Window', 'Ventana', 'Full screen', 'Pantalla completa',
        'Repeat last region', 'Repetir última región', 'Scrolling capture', 'Captura con desplazamiento',
        'Record screen', 'Grabar pantalla', 'Record as GIF', 'Grabar como GIF', 'Stop recording', 'Detener grabación',
        'Text from screen (OCR)', 'Texto de pantalla (OCR)', 'Scan QR code', 'Escanear código QR',
        'Color picker', 'Selector de color', 'Pin to screen', 'Fijar en pantalla', 'Ruler', 'Regla', 'No matches', 'Sin coincidencias',
        'Search', 'Buscar', 'Filter by key or action', 'Filtrar por tecla o acción',
        'move', 'mover', 'select', 'seleccionar', 'back', 'volver', 'scroll', 'desplazarse', 'close', 'cerrar',
        'binding', 'atajo', 'bindings', 'atajos', 'result', 'resultado', 'results', 'resultados',
        'Menus', 'Menús', 'Webapps', 'Aplicaciones web', 'Clipboard', 'Portapapeles', 'Windows', 'Ventanas',
        'Mouse', 'Mouse', 'Focus', 'Foco', 'Move window', 'Mover ventana', 'Stacks', 'Pilas', 'Resize', 'Redimensionar',
        'Workspaces', 'Espacios de trabajo', 'Monitors', 'Monitores', 'System settings', 'Configuración del sistema',
        'terminal', 'Terminal', 'launcher (Flow)', 'Lanzador (Flow)', 'search files (Everything)', 'Buscar archivos (Everything)',
        'browser', 'Navegador', 'file explorer', 'Explorador de archivos', 'music (Spotify / YT Music)', 'Música (Spotify / YT Music)',
        'Obsidian', 'Obsidian', 'calculator', 'Calculadora', 'coding agent', 'Agente de código', 'Winarchy menu', 'Menú de Winarchy',
        'system / power menu', 'Menú del sistema y energía', 'this keybindings list', 'Esta lista de atajos de teclado',
        'native shell: show CPU / RAM in the island', 'Mostrar CPU / RAM en la isla nativa',
        'focus the dock', 'Enfocar el dock',
        'hold to talk to the voice assistant (needs `winarchy assistant on`)', 'Mantener para hablar con el asistente (requiere `winarchy assistant on`)',
        'ChatGPT', 'ChatGPT', 'Claude', 'Claude', 'YouTube', 'YouTube', 'X', 'X', 'Google Calendar', 'Google Calendar', 'WhatsApp', 'WhatsApp',
        'select all', 'Seleccionar todo', 'copy', 'Copiar', 'cut', 'Cortar', 'clipboard history', 'Historial del portapapeles',
        'close window', 'Cerrar ventana', 'monocle (fill the workspace)', 'Monocle (ocupar el espacio de trabajo)',
        'maximize', 'Maximizar', 'minimize', 'Minimizar', 'float / tile', 'Flotar / mosaico', 'pause tiling', 'Pausar mosaico',
        'retile', 'Reaplicar mosaico', 'reload the whole stack', 'Recargar todo el entorno', 'promote to the largest tile', 'Promover al mosaico más grande',
        'next mode (tiling, scrolling, floating)', 'Modo siguiente (mosaico, desplazamiento, flotante)', 'previous mode', 'Modo anterior',
        'next occupied workspace', 'Espacio de trabajo ocupado siguiente', 'previous occupied workspace', 'Espacio de trabajo ocupado anterior',
        'back to the workspace you came from', 'Volver al espacio de trabajo anterior',
        'scroll down = previous occupied workspace', 'Desplazarse hacia abajo: espacio ocupado anterior',
        'scroll up = next occupied workspace', 'Desplazarse hacia arriba: espacio ocupado siguiente',
        'focus the largest tile', 'Enfocar el mosaico más grande', 'mirror layout left / right', 'Invertir distribución izquierda / derecha',
        'mirror layout up / down', 'Invertir distribución arriba / abajo', 'stop tiling this workspace', 'Dejar de organizar este espacio de trabajo',
        'save tile sizes', 'Guardar tamaños de mosaico', 'restore tile sizes', 'Restaurar tamaños de mosaico',
        'dim unfocused windows', 'Atenuar ventanas sin foco', 'stay awake', 'Mantener activo',
        'emoji picker (Win+. is the monitor hotkey here)', 'Selector de emojis (Win+. es la tecla del monitor aquí)',
        'switch tiling / floating layer', 'Cambiar entre mosaico y ventanas flotantes', 'lock the tile in place', 'Bloquear el mosaico en su lugar',
        'lock the session', 'Bloquear la sesión', 'drag to move the window under the cursor', 'Arrastrar para mover la ventana bajo el cursor',
        'drag to resize it (grab the nearest edge or corner)', 'Arrastrar para cambiar el tamaño desde el borde o esquina más cercana',
        'move focus', 'Mover el foco', 'move left', 'Mover a la izquierda', 'move right', 'Mover a la derecha',
        'move up', 'Mover arriba', 'move down', 'Mover abajo', 'stack with the neighbour', 'Apilar con la ventana vecina',
        'take the window out of the stack', 'Sacar la ventana de la pila', 'stack the whole workspace', 'Apilar todo el espacio de trabajo',
        'unstack the whole container', 'Separar toda la pila', 'previous tab', 'Pestaña anterior', 'next tab', 'Pestaña siguiente',
        'go to tab N of the stack', 'Ir a la pestaña N de la pila', 'wider', 'Más ancho', 'narrower', 'Más estrecho',
        'taller', 'Más alto', 'shorter', 'Más bajo', 'wider, a little', 'Un poco más ancho', 'narrower, a little', 'Un poco más estrecho',
        'taller, a little', 'Un poco más alto', 'shorter, a little', 'Un poco más bajo', 'wider, a lot', 'Mucho más ancho',
        'narrower, a lot', 'Mucho más estrecho', 'taller, a lot', 'Mucho más alto', 'shorter, a lot', 'Mucho más bajo',
        'go to workspace N of the monitor under the cursor', 'Ir al espacio N del monitor bajo el cursor',
        'move window to workspace', 'Mover la ventana al espacio de trabajo', 'send window to workspace, stay here', 'Enviar la ventana al espacio de trabajo y quedarse aquí',
        'focus previous monitor', 'Enfocar el monitor anterior', 'focus next monitor', 'Enfocar el monitor siguiente',
        'move window to previous monitor', 'Mover la ventana al monitor anterior', 'move window to next monitor', 'Mover la ventana al monitor siguiente',
        'send window to previous monitor, stay here', 'Enviar la ventana al monitor anterior y quedarse aquí',
        'send window to next monitor, stay here', 'Enviar la ventana al monitor siguiente y quedarse aquí',
        'move workspace to previous monitor', 'Mover el espacio de trabajo al monitor anterior',
        'move workspace to next monitor', 'Mover el espacio de trabajo al monitor siguiente',
        'move window to the monitor on that side', 'Mover la ventana al monitor de ese lado',
        'move workspace to the monitor on that side', 'Mover el espacio de trabajo al monitor de ese lado',
        'region', 'Región', 'active window', 'Ventana activa', 'full screen', 'Pantalla completa',
        'record screen', 'Grabar la pantalla', 'record as GIF', 'Grabar como GIF', 'stop recording', 'Detener la grabación',
        'text from screen (OCR)', 'Texto de pantalla (OCR)', 'colour picker', 'Selector de color', 'QR code scan', 'Escanear código QR',
        'theme gallery', 'Galería de temas', 'next background', 'Siguiente fondo', 'sound', 'Sonido', 'bluetooth', 'Bluetooth',
        'network', 'Red', 'display', 'Pantalla', 'power & sleep', 'Energía y suspensión'
    )
    return (SubStr(WinarchyLocale, 1, 2) = 'es' && es.Has(text)) ? es[text] : text
}

WinarchyLocalize(items) {
    for it in items {
        it.text := WinarchyText(it.text)
        if it.HasOwnProp('hint')
            it.hint := WinarchyText(it.hint)
        if it.HasOwnProp('sub')
            WinarchyLocalize(it.sub)
    }
    return items
}

OnOff(flag) => WinarchyText(FileExist(flag) ? 'On' : 'Off')

WinarchyMenuItems() {
    global GameFlag, AwakeFlag
    return [
        {text:'Apps',          hint:'SUPER+Space',        action:(*)=>ToggleFlowScoped('app ')},
        {text:'System tray',                              action:(*)=>ShellIsland('dock.tray')},
        {text:'Appearance',                               sub: WinarchyAppearanceItems()},
        {text:'Capture',                                  sub: WinarchyCaptureItems()},
        {text:'Tiling',                                   sub: WinarchyTilingItems()},
        {text:'Bar',                                      sub: WinarchyBarItems()},
        {text:'Coding agent',  hint:'SUPER+Ctrl+Shift+A', action:(*)=>Winarchy('agent launch')},
        {text:'Keybindings',   hint:'SUPER+K',            action:(*)=>ToggleKeyOverlay()},
        {text:'Reload stack',  hint:'SUPER+Shift+R',      action:(*)=>Winarchy('reload')},
        {text:WinarchyText('Game mode') ' · ' OnOff(GameFlag), action:(*)=>ToggleGameMode()},
        {text:WinarchyText('Stay awake') ' · ' OnOff(AwakeFlag), hint:'SUPER+Ctrl+I', action:(*)=>ToggleStayAwake()},
        {text:'Doctor',                                   action:(*)=>WinarchyTerminal('doctor')},
        {text:'Check for updates',                        action:(*)=>WinarchyTerminal('update')},
        {text:'System',        hint:'SUPER+Esc',          sub: WinarchySysItems()},
        {text:'Quit Winarchy',                            action:(*)=>QuitStack()} ]
}

WinarchyMenuItemsWithUser() {
    items := WinarchyLocalize(WinarchyMenuItems())
    if IsSet(WinarchyUserMenu) && WinarchyUserMenu is Array {
        for entry in WinarchyUserMenu
            items.InsertAt(items.Length, entry)     ; before Quit, which closes the list
    }
    return items
}

WinarchyAppearanceItems() {
    return [
        {text:'Next theme',      hint:'SUPER+Shift+T',    action:(*)=>Winarchy('theme next')},
        {text:'Theme gallery',    hint:'SUPER+Ctrl+T',     action:(*)=>Winarchy('theme gallery')},
        {text:'Wallpaper gallery',                     action:(*)=>Winarchy('background gallery')},
        {text:'Next background',  hint:'SUPER+Ctrl+Space', action:(*)=>Winarchy('background next')} ]
}

WinarchyTilingItems() {
    return [
        {text:'Layout',                              hint:'SUPER+Shift+L',  sub: WinarchyLayoutItems()},
        {text:'Manage this window',                                action:(*)=>Komorebic('manage')},
        {text:'Unmanage this window',                              action:(*)=>Komorebic('unmanage')},
        {text:'Stop tiling this workspace', hint:'SUPER+Shift+Z',  action:(*)=>Komorebic('toggle-tiling')},
        {text:'New windows: stack / tile',                         action:(*)=>Komorebic('toggle-window-container-behaviour')},
        {text:'Title bars',                                        action:(*)=>Komorebic('toggle-title-bars')},
        {text:'Mouse follows focus',                               action:(*)=>Komorebic('toggle-mouse-follows-focus')},
        {text:'Restore hidden windows',                            action:(*)=>Komorebic('restore-windows')} ]
}

WinarchyLayoutItems() {
    L := (name, glyph, what, cmd) => {text: name, hint: glyph '  ' WinarchyText(what), layout: cmd, action: (*) => SetWorkspaceMode(cmd = 'scrolling' ? 'scrolling' : 'tiling', cmd)}
    return [
        L('BSP', Chr(0x25E9), 'split in halves', 'bsp'),
        L('Grid', Chr(0x25A6), 'even 2' Chr(0xD7) '2 grid', 'grid'),
        L('Columns', Chr(0x25A5), 'side by side', 'columns'),
        L('Rows', Chr(0x25A4), 'stacked rows', 'rows'),
        L('Vertical stack', Chr(0x25E7), 'main left', 'vertical-stack'),
        L('Right main stack', Chr(0x25E8), 'main right', 'right-main-vertical-stack'),
        L('Horizontal stack', Chr(0x2B13), 'main on top', 'horizontal-stack'),
        L('Ultrawide stack', Chr(0x25EB), 'main in centre', 'ultrawide-vertical-stack'),
        L('Scrolling', Chr(0x25A3), 'scrolling strip', 'scrolling'),
        {text: 'Floating', hint: Chr(0x25B3) '  ' WinarchyText('windows stay where you put them'), layout: 'floating', action: (*) => SetWorkspaceMode('floating')},
        {text: 'Next mode', hint: 'SUPER+Shift+L', action: (*) => CycleWorkspaceMode(1)} ]
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
        {text:'Stop recording',         hint:'SUPER+Ctrl+Shift+V', action:(*)=>Winarchy('screenshot stop')},
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
        {text:'Switch user',     action:(*)=>SwitchUserSystem()},
        {text:'Sign out',        action:(*)=>SignOutSystem()},
        {text:'Restart',         action:(*)=>RestartSystem()},
        {text:'Shut down',       action:(*)=>ShutdownSystem()},
        {text:'Project',         action:(*)=>ProjectDisplay()},
        {text:'Settings',        action:(*)=>Run('ms-settings:')} ]
}

ShowMainMenu(*) {
    if !PalClosed('menu')
        PalOpen('menu', WinarchyMenuItemsWithUser(), 'Winarchy')
}

ShowSystemMenu(*) {
    if !PalClosed('system')
        PalOpen('system', WinarchyLocalize(WinarchySysItems()), WinarchyText('System'))
}

RunDialog() {
    ; shell32 ordinal 61 = RunFileDlg: no depende de Explorer
    DllCall(DllCall('GetProcAddress', 'ptr', DllCall('GetModuleHandle', 'str', 'shell32', 'ptr'), 'ptr', 61, 'ptr'), 'ptr', 0, 'ptr', 0, 'ptr', 0, 'ptr', 0, 'ptr', 0, 'uint', 0)
}

ShellIsland(cmd, extra := '') {
    global sessionId
    pipeName := '\\.\pipe\winarchy-shell-' sessionId
    if !DllCall('WaitNamedPipeW', 'str', pipeName, 'uint', 250, 'int')
        return false
    pipe := DllCall('CreateFileW', 'str', pipeName, 'uint', 0x40000000, 'uint', 0, 'ptr', 0, 'uint', 3, 'uint', 0, 'ptr', 0, 'ptr')
    if pipe = -1
        return false
    try {
        payload := '{"v":1,"id":1,"cmd":"' cmd '"' extra '}' Chr(10)
        byteCount := StrPut(payload, 'UTF-8') - 1
        data := Buffer(byteCount + 1)
        StrPut(payload, data, 'UTF-8')
        written := 0
        return DllCall('WriteFile', 'ptr', pipe, 'ptr', data, 'uint', byteCount, 'uint*', &written, 'ptr', 0, 'int') && written = byteCount
    } finally {
        DllCall('CloseHandle', 'ptr', pipe)
    }
}


DockPin(n) {
    ShellIsland('dock.pin', ',"params":{"n":' n '}')
}

; Win abre el Inicio propio de la shell; sin shell que responda queda Flow como lanzador.
OpenStart() {
    if (pid := GetShellPid()) {
        DllCall('user32\AllowSetForegroundWindow', 'uint', pid)
        if ShellIsland('start.toggle')
            return
    }
    ToggleFlow()
}

FocusDock() {
    if (pid := GetShellPid()) {
        DllCall('user32\AllowSetForegroundWindow', 'uint', pid)
        ShellIsland('dock.focus')
    }
}

; Push-to-talk del asistente de voz. Se suelta con el reloj de pulsaciones físicas: no depende del orden
; en que se sueltan Win y CapsLock, ni de que el hotkey "up" sobreviva a un cambio de modificadores.
GetAssistantPid() {
    global AssistantPidFile, sessionId
    if !AssistantPidFile || !FileExist(AssistantPidFile)
        return 0
    try pid := Integer(Trim(FileRead(AssistantPidFile)))
    catch
        return 0
    candidateSession := 0
    if pid < 1 || pid > 4294967295 || !ProcessExist(pid) || !DllCall('ProcessIdToSessionId', 'uint', pid, 'uint*', &candidateSession, 'int') || candidateSession != sessionId
        return 0
    try if StrLower(ProcessGetName(pid)) = 'winarchy-assistant.exe'
        return pid
    return 0
}

AssistantSend(cmd) {
    global sessionId
    try {
        pipe := FileOpen('\\.\pipe\winarchy-assistant-' sessionId, 'w')
        pipe.Write(cmd '`n')
        pipe.Close()
    }
}

AssistantReady() {
    global GameFlag
    return !FileExist(GameFlag) && GetAssistantPid()
}

AssistantPtt() {
    static held := false
    if held
        return
    held := true
    AssistantSend('ptt.down')
    SetTimer(Watch, 40)
    Watch() {
        if GetKeyState('CapsLock', 'P')
            return
        SetTimer(Watch, 0)
        held := false
        AssistantSend('ptt.up')
    }
}

ToggleKeyOverlay(*) {
    if PalClosed('keys')
        return
    sections := ParseKeymap(A_ScriptFullPath, '', '; HOTKEYS')
    if FileExist(A_ScriptDir '\user.ahk')
        for s in ParseKeymap(A_ScriptDir '\user.ahk', 'User')
            sections.Push(s)
    PalOpen('keys', sections, WinarchyText('Keybindings'))
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
            title := RegExReplace(Trim(m[1]), '\s*\(.*\)$')
            cur := {title: startAt != '' ? WinarchyText(title) : title, items: []}
            sections.Push(cur)
            continue
        }
        if !IsObject(cur) || !RegExMatch(line, '^#([+^!]*)([^:\s]+)::(.*)$', &m)
            continue
        mods := m[1], key := m[2], rest := m[3]
        ; 1..0 and the four arrows are listed once
        if RegExMatch(key, '^[02-9]$') || key = 'Right' || key = 'Up' || key = 'Down'
            continue
        if (key = '1')
            key := '1' Chr(0x2026) '0'
        else if (key = 'Left')
            key := Chr(0x2190) ' ' Chr(0x2191) ' ' Chr(0x2192) ' ' Chr(0x2193)
        else if (key = 'Enter')
            key := 'Return'
        else if (key = 'LButton' || key = 'RButton')
            key := (key = 'LButton' ? 'Left' : 'Right') ' drag'
        else
            key := StrUpper(SubStr(key, 1, 1)) SubStr(key, 2)
        desc := RegExMatch(rest, ';\s*(.+)$', &md) ? Trim(md[1]) : Trim(rest)
        if (startAt != '')
            desc := WinarchyText(desc)
        cur.items.Push({
            keys: 'SUPER' (InStr(mods, '!') ? '+Alt' : '') (InStr(mods, '^') ? '+Ctrl' : '') (InStr(mods, '+') ? '+Shift' : '') '+' key,
            desc: StrUpper(SubStr(desc, 1, 1)) SubStr(desc, 2)})
    }
    return sections
}

; --- Palette: themed, searchable popup list (menu, system menu, SUPER+K) -----------
Pal := ''

ThemeColors() {
    c := {bg:'1a1b26', fg:'c0caf5', ac:'7aa2f7', mut:'565f89', font:'JetBrainsMono NF'}
    ini := A_ScriptDir '\theme.ini'
    if FileExist(ini) {
        c.bg  := LTrim(IniRead(ini, 'colors', 'background', c.bg), '#')
        c.fg  := LTrim(IniRead(ini, 'colors', 'foreground', c.fg), '#')
        c.ac  := LTrim(IniRead(ini, 'colors', 'accent', c.ac), '#')
        c.mut := LTrim(IniRead(ini, 'colors', 'muted', c.mut), '#')
        c.font := IniRead(ini, 'font', 'family', c.font)
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

    PalSetFont(g, 's10 bold', c.font)
    crumb := g.Add('Text', Format('x{} y{} w{} h20 c{} 0x4200', pad, pad, w - pad * 2, c.ac), '')
    PalSetFont(g, 's12 norm', c.font)
    g.Add('Text', Format('x{} y{} w24 h30 c{} 0x200', pad, pad + 30, c.mut), Chr(0xF002))
    ed := g.Add('Edit', Format('x{} y{} w{} h30 -E0x200 -Multi Background{} c{}', pad + 28, pad + 34, w - pad * 2 - 28, c.bg, c.fg))
    SendMessage(0x1501, 1, StrPtr(WinarchyText(keys ? 'Filter by key or action' : 'Search')), ed)   ; EM_SETCUEBANNER
    g.Add('Text', Format('x{} y{} w{} h1 Background{}', pad, pad + 68, w - pad * 2, c.mut))

    y0 := pad + 80
    PalSetFont(g, 's11 norm', c.font)
    rows := []
    loop n {
        y := y0 + (A_Index - 1) * rowH
        lbl := g.Add('Text', Format('x{} y{} w{} h{} c{} Background{} 0x4200', pad - 8, y, labelW + 8, rowH, c.fg, c.bg), '')
        hnt := g.Add('Text', Format('x{} y{} w{} h{} c{} Background{} 0x4200 {}', pad + labelW, y, hintW + 8, rowH, c.mut, c.bg, keys ? '' : 'Right'), '')
        rows.Push({label: lbl, hint: hnt, y: y, on: false})
    }
    fy := y0 + n * rowH + 10
    PalSetFont(g, 's9 norm', c.font)
    arrows := Chr(0x2191) Chr(0x2193)
    help := keys ? arrows ' ' WinarchyText('scroll') '    esc ' WinarchyText('close') : arrows ' ' WinarchyText('move') '    ' Chr(0x21B5) ' ' WinarchyText('select') '    esc ' WinarchyText('back')
    g.Add('Text', Format('x{} y{} w{} h20 c{}', pad, fy, labelW, c.fg), help)
    count := g.Add('Text', Format('x{} y{} w{} h20 c{} Right', pad + labelW, fy, hintW, c.fg), '')
    h := fy + 20 + pad - 4

    boxes := []
    loop 4
        boxes.Push(g.Add('Text', 'x0 y0 w1 h1 Hidden Background' c.mut))
    Pal := {gui: g, mode: mode, colors: c, pv: {boxes: boxes, w0: w, h: h, on: false}, items: items, title: title, stack: [], list: [],
        sel: 0, top: 1, rows: rows, rowH: rowH, y0: y0, x0: pad - 8, x1: pad + labelW + hintW + 8,
        edit: ed, crumb: crumb, count: count, mouse: ''}
    ed.OnEvent('Change', (*) => PalRefresh())
    g.OnEvent('Close', (*) => PalClose())
    PalRefresh()

    WorkAreaUnderMouse(&wl, &wt, &wr, &wb)
    g.Show(Format('x{} y{} w{} h{}', wl + (wr - wl - w) // 2, wt + (wb - wt - h) // 2, w, h))
    DllCall('dwmapi\DwmSetWindowAttribute', 'ptr', g.Hwnd, 'int', 33, 'int*', 2, 'int', 4)  ; rounded corners
    Pal.pv.on := false
    PalPreview()
    ed.Focus()
    OnMessage(0x200, PalMouseMove)       ; WM_MOUSEMOVE
    OnMessage(0x202, PalClick)           ; WM_LBUTTONUP
    OnMessage(0x20A, PalWheel)           ; WM_MOUSEWHEEL
    SetTimer(PalWatch, 250)              ; closes when it loses focus
}

; Fallback JetBrainsMono (Nerd Fonts v3 or v2 name); the theme's font wins when installed.
PalSetFont(g, opts, font) {
    g.SetFont(opts, 'JetBrainsMono Nerd Font')
    g.SetFont(opts, 'JetBrainsMono NF')
    g.SetFont(opts, font)
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
    Pal.count.Text := (Pal.mode = 'keys') ? found ' ' WinarchyText(found = 1 ? 'binding' : 'bindings') : (q = '' ? '' : found ' ' WinarchyText(found = 1 ? 'result' : 'results'))
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
        e := (i <= len) ? Pal.list[i] : {text: (k = 1 && !len) ? WinarchyText('No matches') : '', hint: '', empty: true}
        head := e.HasOwnProp('header'), on := (i = Pal.sel)
        if (on != r.on) {
            r.on := on
            r.label.Opt('Background' (on ? c.ac : c.bg))
            r.hint.Opt('Background' (on ? c.ac : c.bg))
        }
        r.label.SetFont((head ? 'bold' : 'norm') ' c' (on ? c.bg : head ? c.ac : e.HasOwnProp('empty') ? c.mut : c.fg))
        r.hint.SetFont('c' (on ? c.bg : c.fg))
        r.label.Text := ' ' e.text
        r.hint.Text := e.hint ' '
    }
    PalPreview()
}

; Layout items grow the palette a side panel that draws four windows tiled that way.
PalPreview() {
    pv := Pal.pv
    e := Pal.sel ? Pal.list[Pal.sel] : ''
    lay := (IsObject(e) && e.HasOwnProp('item') && e.item.HasOwnProp('layout')) ? e.item.layout : ''
    if (lay = '') {
        if pv.on {
            for b in pv.boxes
                b.Visible := false
            Pal.gui.Move(, , pv.w0, pv.h)
            pv.on := false
        }
        return
    }
    if !pv.on {
        Pal.gui.Move(, , pv.w0 + 260, pv.h)
        pv.on := true
    }
    px := pv.w0 + 20, py := Pal.y0 + 6, pw := 220, ph := 150
    for i, b in pv.boxes {
        r := LayoutRects(lay)[i]
        b.Opt('Background' (i = 1 ? Pal.colors.ac : Pal.colors.mut))
        b.Move(px + Round(r[1] * pw) + 2, py + Round(r[2] * ph) + 2, Round(r[3] * pw) - 4, Round(r[4] * ph) - 4)
        b.Visible := true
    }
}

; Four windows as [x, y, w, h] fractions; the first is the main one.
LayoutRects(name) {
    switch name {
        case 'bsp': return [[0, 0, .5, 1], [.5, 0, .5, .5], [.5, .5, .25, .5], [.75, .5, .25, .5]]
        case 'grid': return [[0, 0, .5, .5], [.5, 0, .5, .5], [0, .5, .5, .5], [.5, .5, .5, .5]]
        case 'columns': return [[0, 0, .25, 1], [.25, 0, .25, 1], [.5, 0, .25, 1], [.75, 0, .25, 1]]
        case 'rows': return [[0, 0, 1, .25], [0, .25, 1, .25], [0, .5, 1, .25], [0, .75, 1, .25]]
        case 'vertical-stack': return [[0, 0, .5, 1], [.5, 0, .5, 1 / 3], [.5, 1 / 3, .5, 1 / 3], [.5, 2 / 3, .5, 1 / 3]]
        case 'right-main-vertical-stack': return [[.5, 0, .5, 1], [0, 0, .5, 1 / 3], [0, 1 / 3, .5, 1 / 3], [0, 2 / 3, .5, 1 / 3]]
        case 'horizontal-stack': return [[0, 0, 1, .5], [0, .5, 1 / 3, .5], [1 / 3, .5, 1 / 3, .5], [2 / 3, .5, 1 / 3, .5]]
        case 'ultrawide-vertical-stack': return [[.25, 0, .5, 1], [0, 0, .25, 1], [.75, 0, .25, .5], [.75, .5, .25, .5]]
        case 'scrolling': return [[0, 0, .4, 1], [.4, 0, .4, 1], [.8, 0, .2, 1], [0, 0, 0, 0]]
        case 'floating': return [[.04, .06, .5, .55], [.34, .18, .5, .55], [.16, .36, .5, .55], [.46, .4, .5, .55]]
    }
    return [[0, 0, 1, 1], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]
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

; --- Lista de juegos (games.toml + games.user.toml: exe = "...") ---------------
GameExes := Map()
LoadGames() {
    global GameExes, GamesTomls
    GameExes := Map()
    for file in GamesTomls {
        if !FileExist(file)
            continue
        for line in StrSplit(FileRead(file, 'UTF-8'), '`n') {
            if RegExMatch(line, 'i)^\s*exe\s*=\s*"([^"]+)"', &m) {
                exe := StrLower(m[1])
                if !InStr(exe, '*')                 ; los placeholders con * se ignoran
                    GameExes[exe] := true
            }
        }
    }
}
LoadGames()

GamesStamp() {
    s := ''
    for f in GamesTomls
        try s .= FileGetTime(f) ';'
    return s
}
GamesSeen := GamesStamp()

; --- Game-mode watcher (≥1 s, costo CPU despreciable) --------------------------
GameModeActive := false
SetTimer(GameWatch, 1000)

GameWatch() {
    global GameModeActive, GamesSeen
    active := false

    ; altas en caliente (`game-mode add`): recargar solo si cambió algún toml
    if ((now := GamesStamp()) != GamesSeen) {
        GamesSeen := now
        LoadGames()
    }

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
                ; cubre el monitor donde está (chequeo sobre todos los monitores)
                loop MonitorGetCount() {
                    MonitorGet(A_Index, &l, &t, &r, &b)
                    if (x <= l && y <= t && x + w >= r && y + h >= b) {
                        exe := StrLower(WinGetProcessName(hwnd))
                        ; nunca tratar shell/escritorio como juego
                        if exe != 'explorer.exe' && exe != 'searchhost.exe' && exe != 'yasb.exe' && exe != 'winarchy-shell.exe'
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
    ; trackea estado. Animaciones ya apagadas siempre (config).
    if (active && !GameModeActive) {
        GameModeActive := true
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
    try cur := Trim(FileRead(AccentMarker, 'UTF-8'))
    catch
        return   ; theme set lo está reescribiendo; el próximo tick lo lee
    if (hex = cur) {
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
#+Enter::Run(DefaultBrowser())                   ; browser
#+b::Run(DefaultBrowser())
#+f::OpenFiles()                                 ; file explorer
#e::OpenFiles()                                   ; Files if installed, else Explorer
; Win+N stays with Windows (notification center; the YASB bell mirrors it)
#+m::LaunchMusic()                               ; music (Spotify / YT Music)
#+o::LaunchObsidian()                            ; Obsidian
#!r::RunDialog()                                 ; Run dialog (Win+R is retile)
#+q::Run('calc.exe')                             ; calculator
#^+a::Winarchy('agent launch')                   ; coding agent

; --- Menus ----------------------------------------------------------------------
#!Space::ShowMainMenu()                          ; Winarchy menu
#Esc::ShowSystemMenu()                           ; system / power menu
#k::ToggleKeyOverlay()                           ; this keybindings list
#i::ShellIsland('island.expand')                 ; native shell: show CPU / RAM in the island
#!t::FocusDock()                                 ; focus the dock
#HotIf AssistantReady()
#CapsLock::AssistantPtt()                        ; hold to talk to the voice assistant (needs `winarchy assistant on`)
#HotIf

; --- Webapps --------------------------------------------------------------------
#+a::WebApp('https://chatgpt.com')               ; ChatGPT
#+!a::WebApp('https://claude.ai')                ; Claude
#+y::WebApp('https://youtube.com')               ; YouTube
#+x::WebApp('https://x.com')                     ; X
#+c::WebApp('https://calendar.google.com')       ; Google Calendar
#+!g::WebApp('https://web.whatsapp.com')         ; WhatsApp

; --- Clipboard ------------------------------------------------------------------
#a::Send('^a')                                   ; select all
#c::Send(InTerminal() ? '^+c' : '^c')            ; copy
$#v::Send(InTerminal() ? '^+v' : '^v')           ; paste ($: SUPER+Ctrl+V sends Win+V and must not land here)
#x::Send('^x')                                   ; cut
#^v::Send('#v')                                  ; clipboard history

; --- Windows -------------------------------------------------------------------
#w::CloseWindow()                                ; close window
#q::CloseWindow()
#f::Komorebic('toggle-monocle')                  ; monocle (fill the workspace)
#!f::Komorebic('toggle-maximize')                ; maximize
#m::Komorebic('minimize')                        ; minimize
#t::Komorebic('toggle-float')                    ; float / tile
#p::Komorebic('toggle-pause')                    ; pause tiling
#r::Komorebic('retile')                          ; retile
#+r::Winarchy('reload')                          ; reload the whole stack
#^+Enter::Komorebic('promote')                   ; promote to the largest tile
#+l::CycleWorkspaceMode(1)                       ; next mode (tiling, scrolling, floating)
#!+l::CycleWorkspaceMode(-1)                      ; previous mode
#Tab::CycleOccupied(1)                           ; next occupied workspace
#+Tab::CycleOccupied(-1)                         ; previous occupied workspace
#^Tab::Komorebic('focus-last-workspace')         ; back to the workspace you came from
#WheelDown::CycleOccupied(-1, false)             ; scroll down = previous occupied workspace
#WheelUp::CycleOccupied(1, false)                ; scroll up = next occupied workspace
#^Enter::Komorebic('promote-focus')              ; focus the largest tile
#+h::Komorebic('flip-layout horizontal')         ; mirror layout left / right
#+j::Komorebic('flip-layout vertical')           ; mirror layout up / down
#+z::Komorebic('toggle-tiling')                  ; stop tiling this workspace
#!Home::Komorebic('quick-save-resize')           ; save tile sizes
#Home::Komorebic('quick-load-resize')            ; restore tile sizes
#+d::Komorebic('toggle-transparency')            ; dim unfocused windows
#^i::ToggleStayAwake()                           ; stay awake
#^e::Send('#;')                                  ; emoji picker (Win+. is the monitor hotkey here)
#^f::Komorebic('toggle-workspace-layer')         ; switch tiling / floating layer
#^+l::Komorebic('toggle-lock')                   ; lock the tile in place
#^l::DllCall('LockWorkStation')                  ; lock the session

; --- Mouse ---------------------------------------------------------------------
#LButton::WindowDrag(true)                       ; drag to move the window under the cursor
#RButton::WindowDrag(false)                      ; drag to resize it (grab the nearest edge or corner)

; --- Focus ---------------------------------------------------------------------
#Left::HorizontalNav('focus', 'left')            ; move focus
#Right::HorizontalNav('focus', 'right')
#Up::Komorebic('focus up')
#Down::Komorebic('focus down')

; --- Move window ---------------------------------------------------------------
#+Left::HorizontalNav('move', 'left')            ; move window
#+Right::HorizontalNav('move', 'right')
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
#!g::Komorebic('unstack')
#!,::Komorebic('cycle-stack previous')           ; previous tab
#!.::Komorebic('cycle-stack next')               ; next tab
#!+Tab::Komorebic('cycle-stack previous')
#!Tab::Komorebic('cycle-stack next')
#^Left::Komorebic('cycle-stack previous')
#^Right::Komorebic('cycle-stack next')
#!WheelUp::Komorebic('cycle-stack previous')
#!WheelDown::Komorebic('cycle-stack next')
#!1::Komorebic('focus-stack-window 0')           ; go to tab N of the stack
#!2::Komorebic('focus-stack-window 1')
#!3::Komorebic('focus-stack-window 2')
#!4::Komorebic('focus-stack-window 3')
#!5::Komorebic('focus-stack-window 4')

; --- Resize --------------------------------------------------------------------
#=::Komorebic('resize-axis horizontal increase') ; wider
#-::Komorebic('resize-axis horizontal decrease') ; narrower
#+=::Komorebic('resize-axis vertical increase')  ; taller
#+-::Komorebic('resize-axis vertical decrease')  ; shorter
#!=::ResizeStep('horizontal increase', 15)       ; wider, a little
#!-::ResizeStep('horizontal decrease', 15)       ; narrower, a little
#!+=::ResizeStep('vertical increase', 15)        ; taller, a little
#!+-::ResizeStep('vertical decrease', 15)        ; shorter, a little
#^=::ResizeStep('horizontal increase', 150)      ; wider, a lot
#^-::ResizeStep('horizontal decrease', 150)      ; narrower, a lot
#^+=::ResizeStep('vertical increase', 150)       ; taller, a lot
#^+-::ResizeStep('vertical decrease', 150)       ; shorter, a lot

; --- Workspaces (komorebi, not Windows virtual desktops) -------------------------
#1::FocusWorkspace(0)                            ; go to workspace N of the monitor under the cursor
#2::FocusWorkspace(1)
#3::FocusWorkspace(2)
#4::FocusWorkspace(3)
#5::FocusWorkspace(4)
#6::FocusWorkspace(5)
#7::FocusWorkspace(6)
#8::FocusWorkspace(7)
#9::FocusWorkspace(8)
#0::FocusWorkspace(9)

#^+1::DockPin(1)                                  ; open or focus dock app N
#^+2::DockPin(2)
#^+3::DockPin(3)
#^+4::DockPin(4)
#^+5::DockPin(5)
#^+6::DockPin(6)
#^+7::DockPin(7)
#^+8::DockPin(8)
#^+9::DockPin(9)
#+1::Komorebic('move-to-workspace 0')            ; move window to workspace
#+2::Komorebic('move-to-workspace 1')
#+3::Komorebic('move-to-workspace 2')
#+4::Komorebic('move-to-workspace 3')
#+5::Komorebic('move-to-workspace 4')
#+6::Komorebic('move-to-workspace 5')
#+7::Komorebic('move-to-workspace 6')
#+8::Komorebic('move-to-workspace 7')
#+9::Komorebic('move-to-workspace 8')
#+0::Komorebic('move-to-workspace 9')

#^1::Komorebic('send-to-workspace 0')            ; send window to workspace, stay here
#^2::Komorebic('send-to-workspace 1')
#^3::Komorebic('send-to-workspace 2')
#^4::Komorebic('send-to-workspace 3')
#^5::Komorebic('send-to-workspace 4')
#^6::Komorebic('send-to-workspace 5')
#^7::Komorebic('send-to-workspace 6')
#^8::Komorebic('send-to-workspace 7')
#^9::Komorebic('send-to-workspace 8')
#^0::Komorebic('send-to-workspace 9')

#+!1::Komorebic('send-to-workspace 0')
#+!2::Komorebic('send-to-workspace 1')
#+!3::Komorebic('send-to-workspace 2')
#+!4::Komorebic('send-to-workspace 3')
#+!5::Komorebic('send-to-workspace 4')
#+!6::Komorebic('send-to-workspace 5')
#+!7::Komorebic('send-to-workspace 6')
#+!8::Komorebic('send-to-workspace 7')
#+!9::Komorebic('send-to-workspace 8')
#+!0::Komorebic('send-to-workspace 9')

; --- Monitors ------------------------------------------------------------------
#,::Komorebic('cycle-monitor previous')    ; focus previous monitor
#.::Komorebic('cycle-monitor next')        ; focus next monitor
#+,::Komorebic('cycle-move-to-monitor previous')    ; move window to previous monitor
#+.::Komorebic('cycle-move-to-monitor next')        ; move window to next monitor
#^,::Komorebic('cycle-send-to-monitor previous') ; send window to previous monitor, stay here
#^.::Komorebic('cycle-send-to-monitor next')     ; send window to next monitor, stay here
#!+,::Komorebic('cycle-move-workspace-to-monitor previous')  ; move workspace to previous monitor
#!+.::Komorebic('cycle-move-workspace-to-monitor next')      ; move workspace to next monitor
#+^Left::MonitorToward('l', 'move-to-monitor')               ; move window to the monitor on that side
#+^Right::MonitorToward('r', 'move-to-monitor')
#+^Up::MonitorToward('u', 'move-to-monitor')
#+^Down::MonitorToward('d', 'move-to-monitor')
#+!Left::MonitorToward('l', 'move-workspace-to-monitor')     ; move workspace to the monitor on that side
#+!Right::MonitorToward('r', 'move-workspace-to-monitor')
#+!Up::MonitorToward('u', 'move-workspace-to-monitor')
#+!Down::MonitorToward('d', 'move-workspace-to-monitor')
^!Tab::Komorebic('cycle-monitor next')                       ; focus next monitor
^!+Tab::Komorebic('cycle-monitor previous')                  ; focus previous monitor

; --- Capture (ShareX) ------------------------------------------------------------
#+s::Sharex('RectangleRegion')                   ; region
#+w::Sharex('ActiveWindow')                      ; active window
#+p::Sharex('PrintScreen')                       ; full screen
#+v::Sharex('ScreenRecorder')                    ; record screen
#+g::Sharex('ScreenRecorderGIF')                 ; record as GIF
#^+v::Sharex('StopScreenRecording')              ; stop recording
#^o::Sharex('OCR')                               ; text from screen (OCR)
#^PrintScreen::Sharex('OCR')
#PrintScreen::Sharex('ScreenColorPicker')        ; colour picker
#^q::Sharex('QRCodeScanRegion')                  ; scan QR code

; --- Themes --------------------------------------------------------------------
#+t::Winarchy('theme next')                      ; next theme
#^t::Winarchy('theme gallery')                   ; theme gallery
#+^Space::Winarchy('theme gallery')
#^Space::Winarchy('background next')             ; next background
#^w::Winarchy('background next')

; --- System settings -------------------------------------------------------------
#^a::Run('ms-settings:sound')       ; sound
#^b::Run('ms-settings:bluetooth')   ; bluetooth
#^n::Run('ms-settings:network')     ; network
#^d::Run('ms-settings:display')     ; display
#^p::Run('ms-settings:powersleep')  ; power & sleep

; User overrides: config\ahk\user.ahk (untracked). Its hotkeys show in SUPER+K under
; "User", or under their own "; --- Title ---" headers.
#HotIf
#Include *i %A_ScriptDir%\user.ahk

OpenFiles() {
    exe := EnvGet('LOCALAPPDATA') '\Programs\winarchy-files\winarchy-files.exe'
    Run(FileExist(exe) ? '"' exe '"' : 'explorer.exe')
}
