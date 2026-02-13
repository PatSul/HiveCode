param(
    [string[]]$Crates = @(
        "hive_core",
        "hive_fs",
        "hive_terminal",
        "hive_ai",
        "hive_agents",
        "hive_assistant",
        "hive_blockchain",
        "hive_integrations",
        "hive_learn",
        "hive_shield",
        "hive_ui",
        "hive_ui_core",
        "hive_ui_panels",
        "hive_app"
    ),
    [int]$PerCrateTimeoutSec = 900,
    [string]$TargetDir = "target\ci",
    [switch]$FailFast,
    [switch]$VerboseCargo
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Stop-RustBuildProcesses {
    $names = @("cargo", "rustc", "rustdoc", "sccache")
    $running = Get-Process -ErrorAction SilentlyContinue |
        Where-Object { $names -contains $_.ProcessName }

    foreach ($proc in $running) {
        try {
            taskkill /PID $proc.Id /T /F *> $null
        } catch {
        }
    }
}

$workspaceRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $workspaceRoot

$normalizedCrates = New-Object System.Collections.Generic.List[string]
foreach ($entry in $Crates) {
    if ([string]::IsNullOrWhiteSpace($entry)) {
        continue
    }

    $parts = $entry -split ","
    foreach ($part in $parts) {
        $name = $part.Trim(" `"'")
        if (-not [string]::IsNullOrWhiteSpace($name)) {
            $normalizedCrates.Add($name)
        }
    }
}
$Crates = $normalizedCrates

Write-Output "Workspace: $workspaceRoot"
Write-Output "Target dir: $TargetDir"
Write-Output "Per-crate timeout: ${PerCrateTimeoutSec}s"

Stop-RustBuildProcesses

$results = New-Object System.Collections.Generic.List[object]

foreach ($crate in $Crates) {
    Stop-RustBuildProcesses
    Write-Output ""
    Write-Output "=== RUN $crate ==="

    $mode = if ($crate -eq "hive_ui_panels") { "check" } else { "test" }
    $args = if ($mode -eq "test") {
        @("test", "--target-dir", $TargetDir, "-p", $crate)
    } else {
        @("check", "--target-dir", $TargetDir, "-p", $crate, "--lib")
    }
    if (-not $VerboseCargo) {
        $args = @($args[0], "--quiet") + $args[1..($args.Length - 1)]
    }
    if ($mode -eq "test") {
        $args += @("--", "--test-threads=1")
    }

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath "cargo" -ArgumentList ($args -join " ") -NoNewWindow -PassThru
    $finished = $proc.WaitForExit($PerCrateTimeoutSec * 1000)

    if (-not $finished) {
        taskkill /PID $proc.Id /T /F *> $null
        $sw.Stop()
        $results.Add([pscustomobject]@{
                crate  = $crate
                mode   = $mode
                status = "TIMEOUT"
                secs   = [math]::Round($sw.Elapsed.TotalSeconds, 1)
            })
        Write-Output ("=== TIMEOUT {0}: {1:n1}s ===" -f $crate, $sw.Elapsed.TotalSeconds)
        if ($FailFast) {
            break
        }
        continue
    }

    $sw.Stop()
    if ($proc.ExitCode -eq 0) {
        $status = "PASS"
    } else {
        $status = "FAIL($($proc.ExitCode))"
    }
    $results.Add([pscustomobject]@{
            crate  = $crate
            mode   = $mode
            status = $status
            secs   = [math]::Round($sw.Elapsed.TotalSeconds, 1)
        })

    Write-Output ("=== {0} {1}: {2:n1}s ===" -f $status, $crate, $sw.Elapsed.TotalSeconds)

    if ($proc.ExitCode -ne 0 -and $FailFast) {
        break
    }
}

Write-Output ""
Write-Output "=== SUMMARY ==="
$results | Format-Table -AutoSize

$hasFailure = $results | Where-Object { $_.status -notlike "PASS" }
if ($hasFailure) {
    exit 1
}
exit 0
