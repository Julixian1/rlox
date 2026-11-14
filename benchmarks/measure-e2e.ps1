param(
    [string]$Program = "",
    [string]$Backend = "--vm",
    [int]$Warmup = 3,
    [int]$Reps = 10,
    [string]$Csv = ""
)

$ErrorActionPreference = "Stop"

$BinPath = ".\target\release\rlox.exe"
if (-not (Test-Path $BinPath)) {
    Write-Host "Compilando rlox en modo release..." -ForegroundColor Cyan
    cargo build --release --bin rlox
}

$Files = @()
if ($Program -ne "") {
    $Files += (Get-Item $Program)
} else {
    $Files = Get-ChildItem "benchmarks\programs\*.lox" | Sort-Object Name
}

function Get-Quantile($arr, $q) {
    $n = $arr.Count
    if ($n -eq 0) { return 0 }
    if ($n -eq 1) { return $arr[0] }
    $pos = ($n - 1) * $q
    $base = [Math]::Floor($pos)
    $rest = $pos - $base
    if ($base + 1 -lt $n) {
        return $arr[$base] + $rest * ($arr[$base + 1] - $arr[$base])
    } else {
        return $arr[$base]
    }
}

$Results = @()

Write-Host "=== Medicion End-to-End (PowerShell / Process Lifetime) ===" -ForegroundColor Green
Write-Host "Configuracion: Backend='$Backend', Warmup=$Warmup, Reps=$Reps"
Write-Host ("{0,-18} {1,12} {2,10} {3,10} {4,10}" -f "Benchmark", "Mediana(ms)", "IQR(ms)", "Min(ms)", "Max(ms)")
Write-Host ("-" * 66)

foreach ($file in $Files) {
    $name = $file.BaseName

    # Warmup
    for ($i = 0; $i -lt $Warmup; $i++) {
        if ($Backend -ne "") {
            $null = & $BinPath $Backend $file.FullName 2>&1
        } else {
            $null = & $BinPath $file.FullName 2>&1
        }
    }

    # Reps
    $times = @()
    $failed = $false
    for ($i = 0; $i -lt $Reps; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        if ($Backend -ne "") {
            $output = & $BinPath $Backend $file.FullName 2>&1
        } else {
            $output = & $BinPath $file.FullName 2>&1
        }
        $sw.Stop()
        if ($LASTEXITCODE -ne 0) {
            $failed = $true
        }
        $times += $sw.Elapsed.TotalMilliseconds
    }

    if ($failed) {
        Write-Host ("{0,-18} {1,12}" -f $name, "NO SOPORTADO") -ForegroundColor Yellow
        continue
    }

    $sorted = $times | Sort-Object
    $med = Get-Quantile $sorted 0.50
    $q1 = Get-Quantile $sorted 0.25
    $q3 = Get-Quantile $sorted 0.75
    $iqr = $q3 - $q1
    $min = $sorted[0]
    $max = $sorted[-1]

    $medStr = "{0:F2}" -f $med
    $iqrStr = "{0:F2}" -f $iqr
    $minStr = "{0:F2}" -f $min
    $maxStr = "{0:F2}" -f $max

    Write-Host ("{0,-18} {1,12} {2,10} {3,10} {4,10}" -f $name, $medStr, $iqrStr, $minStr, $maxStr)

    $Results += [PSCustomObject]@{
        benchmark = $name
        backend   = if ($Backend -eq "") { "tree" } else { $Backend.TrimStart('-') }
        reps      = $Reps
        warmup    = $Warmup
        med_ms    = [Math]::Round($med, 2)
        iqr_ms    = [Math]::Round($iqr, 2)
        min_ms    = [Math]::Round($min, 2)
        max_ms    = [Math]::Round($max, 2)
    }
}

if ($Csv -ne "") {
    $dir = Split-Path $Csv -Parent
    if ($dir -and (-not (Test-Path $dir))) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    $Results | Export-Csv -Path $Csv -NoTypeInformation
    Write-Host "`nResultados E2E guardados en: $Csv" -ForegroundColor Green
}
