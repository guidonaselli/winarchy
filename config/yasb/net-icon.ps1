# net-icon.ps1 — indicador de red para el CustomWidget "wifi" de YASB.
#
# Decide el glifo de red de forma SINCRONICA (sin el WiFiWorker/winrt de
# yasb.wifi.WifiWidget, que deja el placeholder {wifi_icon} literal cuando no
# puede leer el adaptador: red abajo en el arranque o radio WiFi off). Siempre
# devuelve un glifo, con fallback a "desconectado", asi que la barra nunca
# muestra la plantilla cruda.
#
# Prioridad: Ethernet (link cableado, el efectivo si esta arriba) > WiFi por
# fuerza de senal > desconectado. Glifos Material Design (Nerd Font), por
# codepoint para no depender de la codificacion del archivo.
#
# Salida: un unico glifo UTF-8 a stdout (YASB lo decodifica utf-8 -> {data}).

# Solo aplica cuando stdout está redirigido (como lo corre YASB); en consola
# interactiva el setter tira y no hace falta.
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$ICON_ETH  = 0xF0200                                      # ethernet
$ICON_WIFI = @(0xF092E, 0xF091F, 0xF0922, 0xF0925, 0xF0928)  # 0=off .. 4=full

function Glyph([int]$cp) { [System.Char]::ConvertFromUtf32($cp) }

$cp = $ICON_WIFI[0]  # fallback: desconectado

try {
    $adapters = Get-NetAdapter -Physical -ErrorAction Stop

    $eth = $adapters | Where-Object {
        $_.MediaType -eq '802.3' -and $_.Status -eq 'Up' -and
        $_.InterfaceDescription -notmatch 'Bluetooth'
    } | Select-Object -First 1

    if ($eth) {
        $cp = $ICON_ETH
    }
    else {
        $wifi = $adapters | Where-Object {
            $_.MediaType -eq 'Native 802.11' -and $_.Status -eq 'Up'
        } | Select-Object -First 1

        if ($wifi) {
            $bars = 4
            try {
                $out = (netsh wlan show interfaces 2>$null) -join "`n"
                $m = [regex]::Match($out, '(\d{1,3})\s*%')
                if ($m.Success) {
                    $sig = [int]$m.Groups[1].Value
                    $bars =
                        if ($sig -le 0)  { 0 }
                        elseif ($sig -lt 25) { 1 }
                        elseif ($sig -lt 50) { 2 }
                        elseif ($sig -lt 75) { 3 }
                        else { 4 }
                }
            } catch { }
            $cp = $ICON_WIFI[$bars]
        }
    }
} catch { }

[Console]::Out.Write((Glyph $cp))
