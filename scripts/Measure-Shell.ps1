# Measure-Shell.ps1 - read-only resource sampler for the Winarchy shell benchmark.
# Samples processes by name for -Seconds and prints one JSON object with working set,
# private bytes, handles, threads and average CPU % of one core (deltas of TotalProcessorTime).
# It only reads process counters: it never starts, stops or touches any process.
param(
    [string[]]$Name = @('winarchy-shell', 'explorer'),
    [int]$Seconds = 60,
    [int]$IntervalSeconds = 2
)
$ErrorActionPreference = 'Stop'
$Name = @($Name | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$cores = [Environment]::ProcessorCount
$samples = @{}
$started = Get-Date
$first = @{}
$last = @{}
while (((Get-Date) - $started).TotalSeconds -lt $Seconds) {
    foreach ($n in $Name) {
        foreach ($p in @(Get-Process -Name $n -ErrorAction SilentlyContinue)) {
            $key = "$n#$($p.Id)"
            if (-not $samples.ContainsKey($key)) { $samples[$key] = [System.Collections.Generic.List[object]]::new(); $first[$key] = @{ cpu = $p.TotalProcessorTime.TotalSeconds; at = Get-Date } }
            $samples[$key].Add([pscustomobject]@{ ws = $p.WorkingSet64; priv = $p.PrivateMemorySize64; handles = $p.HandleCount; threads = $p.Threads.Count })
            $last[$key] = @{ cpu = $p.TotalProcessorTime.TotalSeconds; at = Get-Date }
        }
    }
    Start-Sleep -Seconds $IntervalSeconds
}
$result = [ordered]@{ seconds = $Seconds; logical_cores = $cores; processes = @() }
foreach ($key in $samples.Keys | Sort-Object) {
    $s = $samples[$key]
    $wall = ($last[$key].at - $first[$key].at).TotalSeconds
    $cpu = if ($wall -gt 0) { ($last[$key].cpu - $first[$key].cpu) / $wall * 100 } else { 0 }
    $ws = ($s.ws | Measure-Object -Average -Maximum)
    $result.processes += [ordered]@{
        process = $key
        samples = $s.Count
        working_set_mb_avg = [math]::Round($ws.Average / 1MB, 1)
        working_set_mb_max = [math]::Round($ws.Maximum / 1MB, 1)
        private_mb_avg = [math]::Round((($s.priv | Measure-Object -Average).Average) / 1MB, 1)
        handles_avg = [int](($s.handles | Measure-Object -Average).Average)
        threads_avg = [int](($s.threads | Measure-Object -Average).Average)
        cpu_percent_of_one_core = [math]::Round($cpu, 2)
    }
}
$result | ConvertTo-Json -Depth 4
