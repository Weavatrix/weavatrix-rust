$ErrorActionPreference = 'Continue'
$Repo = 'C:\Users\SergiiZiborov\Documents\GitHub\MyProjects\weavatrix-rust'
Set-Location $Repo
$Wx = Join-Path $Repo 'target\release\weavatrix-rust.exe'

function Get-Median([double[]]$Values) {
    $s = $Values | Sort-Object
    if ($s.Count -eq 0) { return $null }
    if ($s.Count % 2 -eq 1) { return $s[[int]($s.Count / 2)] }
    return ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2.0
}

function Invoke-TimedRun {
    param(
        [scriptblock]$Block,
        [int]$Warmup = 0,
        [int]$Samples = 3
    )
    for ($w = 0; $w -lt $Warmup; $w++) {
        $null = & $Block 2>&1 | Out-Null
    }
    $ms = @()
    $lastOut = $null
    $lastErr = $null
    $lastExit = 0
    for ($i = 0; $i -lt $Samples; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $out = & $Block 2>&1
        $sw.Stop()
        $ms += [double]$sw.Elapsed.TotalMilliseconds
        $lastOut = $out
        $lastExit = $LASTEXITCODE
    }
    return @{
        samplesMs = $ms
        medianMs  = Get-Median $ms
        lastOutput = ($lastOut | Out-String).Trim()
        exitCode = $lastExit
    }
}

function Summarize-Output([string]$Text, [int]$Max = 240) {
    if (-not $Text) { return '(no stdout)' }
    $one = ($Text -split "`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 3) -join ' | '
    if ($one.Length -gt $Max) { $one = $one.Substring(0, $Max) + '…' }
    return $one
}

$results = [ordered]@{}
$artifacts = [ordered]@{}

# a) analyze
Write-Host '=== weavatrix analyze ==='
$r = Invoke-TimedRun -Samples 3 -Block {
    & $Wx analyze . 2>&1
}
$results['weavatrix_analyze'] = @{
    command   = "$Wx analyze ."
    version   = (& $Wx --version 2>&1 | Out-String).Trim()
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = Summarize-Output $r.lastOutput
    exitCode  = $r.exitCode
}

# b) graph_stats (cold process x3 + second consecutive sample on last run captured separately)
Write-Host '=== weavatrix graph_stats ==='
$gsNotes = @()
$r = Invoke-TimedRun -Samples 3 -Block {
    $o = & $Wx tool graph_stats . --compact 2>&1
    $gsNotes += (Summarize-Output ($o | Out-String))
    $o
}
$results['weavatrix_graph_stats'] = @{
    command   = "$Wx tool graph_stats . --compact"
    version   = (& $Wx --version 2>&1 | Out-String).Trim()
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = Summarize-Output $r.lastOutput
    exitCode  = $r.exitCode
}
# second consecutive graph_stats in fresh process (proxy for repeat open+stats)
Write-Host '=== weavatrix graph_stats consecutive (2nd in same invocation chain) ==='
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$o1 = & $Wx tool graph_stats . --compact 2>&1 | Out-Null
$sw.Stop()
$firstMs = [double]$sw.Elapsed.TotalMilliseconds
$sw2 = [System.Diagnostics.Stopwatch]::StartNew()
$o2 = & $Wx tool graph_stats . --compact 2>&1
$sw2.Stop()
$secondMs = [double]$sw2.Elapsed.TotalMilliseconds
$results['weavatrix_graph_stats_consecutive_process'] = @{
    command   = 'two back-to-back CLI invocations (not in-process)'
    note      = 'First call ms then second call ms in same shell; not same process.'
    firstMs   = $firstMs
    secondMs  = $secondMs
    lastOutput = Summarize-Output ($o2 | Out-String)
}
try {
    $j = $r.lastOutput | ConvertFrom-Json -ErrorAction Stop
    $artifacts['graph_stats'] = $j
} catch {
    $artifacts['graph_stats_raw'] = $r.lastOutput
}

# c) rg --files
Write-Host '=== rg --files ==='
$rgV = (rg --version 2>&1 | Select-Object -First 1 | Out-String).Trim()
$r = Invoke-TimedRun -Samples 3 -Block {
    rg --files --glob '!target/**' --glob '!.git/**' . 2>&1
}
$fileCount = ($r.lastOutput -split "`n" | Where-Object { $_.Trim() -ne '' }).Count
$artifacts['rg_files_count'] = $fileCount
$results['rg_files'] = @{
    command   = "rg --files --glob '!target/**' --glob '!.git/**' ."
    version   = $rgV
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = "$fileCount file paths listed"
    exitCode  = $r.exitCode
}

# d) rg fn search
Write-Host '=== rg fn search ==='
$r = Invoke-TimedRun -Samples 3 -Block {
    rg -n 'fn ' --glob '!target/**' --glob '!.git/**' . 2>&1
}
$matchLines = ($r.lastOutput -split "`n" | Where-Object { $_.Trim() -ne '' }).Count
$artifacts['rg_fn_match_lines'] = $matchLines
$results['rg_fn_lines'] = @{
    command   = "rg -n `"fn `" --glob '!target/**' --glob '!.git/**' ."
    version   = $rgV
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = "$matchLines lines with matches"
    exitCode  = $r.exitCode
}

# e) git ls-files
Write-Host '=== git ls-files ==='
$gitV = (git --version 2>&1 | Out-String).Trim()
$r = Invoke-TimedRun -Samples 3 -Block {
    (git ls-files --cached 2>&1 | Measure-Object -Line).Lines
}
$results['git_ls_files'] = @{
    command   = 'git ls-files --cached | Measure-Object -Line'
    version   = $gitV
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = "$($r.lastOutput) tracked files (line count)"
    exitCode  = 0
}
$artifacts['git_tracked_files'] = [int]$r.lastOutput

# f) tokei
Write-Host '=== tokei ==='
$tokeiV = (tokei --version 2>&1 | Out-String).Trim()
$r = Invoke-TimedRun -Samples 3 -Block {
    tokei . -e target 2>&1
}
$results['tokei'] = @{
    command   = 'tokei . -e target'
    version   = $tokeiV
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = Summarize-Output $r.lastOutput
    exitCode  = $r.exitCode
}
$artifacts['tokei_output'] = $r.lastOutput

# g) scc skipped
$results['scc'] = @{
    skipped = $true
    reason  = 'crates.io scc is a library; boyter/scc has no Cargo.toml (Go tool). Not installed.'
}

# h) ast-grep
Write-Host '=== ast-grep ==='
$agCmd = 'npx -p @ast-grep/cli ast-grep scan -l rust -p ''fn $NAME($$$)'' .'
$r = Invoke-TimedRun -Warmup 1 -Samples 3 -Block {
    npx -p @ast-grep/cli ast-grep scan -l rust -p 'fn $NAME($$$)' . 2>&1
}
$agVersion = (npx -p @ast-grep/cli ast-grep --version 2>&1 | Out-String).Trim()
$agMatches = ($r.lastOutput -split "`n" | Where-Object { $_.Trim() -ne '' }).Count
$results['ast_grep'] = @{
    command   = $agCmd
    version   = $agVersion
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = if ($r.exitCode -ne 0) { Summarize-Output $r.lastOutput } else { "~$agMatches output lines (structural fn matches)" }
    exitCode  = $r.exitCode
}

# i) repomix
Write-Host '=== repomix ==='
$repOut = Join-Path $Repo 'benchmark-results\_repomix_tmp.xml'
if (Test-Path $repOut) { Remove-Item $repOut -Force }
$repomixV = (npx --yes repomix --version 2>&1 | Out-String).Trim()
$r = Invoke-TimedRun -Warmup 1 -Samples 3 -Block {
    npx --yes repomix --quiet --ignore 'target/**' -o $repOut . 2>&1
}
$repSize = if (Test-Path $repOut) { (Get-Item $repOut).Length } else { 0 }
$results['repomix'] = @{
    command   = "npx --yes repomix --quiet --ignore 'target/**' -o $repOut ."
    version   = $repomixV
    samplesMs = $r.samplesMs
    medianMs  = $r.medianMs
    note      = "Packed repo to XML; output bytes=$repSize"
    exitCode  = $r.exitCode
}
if (Test-Path $repOut) { Remove-Item $repOut -Force }

# j) gitingest
Write-Host '=== gitingest ==='
$gi = Invoke-TimedRun -Warmup 1 -Samples 1 -Block {
    npx --yes gitingest --help 2>&1
}
if ($gi.exitCode -ne 0 -or ($gi.lastOutput -match '404|could not determine|ENOENT')) {
    $results['gitingest'] = @{
        skipped = $true
        reason  = 'npx gitingest CLI not available or failed help probe'
        probe   = Summarize-Output $gi.lastOutput
    }
} else {
    $r = Invoke-TimedRun -Warmup 1 -Samples 3 -Block {
        npx --yes gitingest $Repo 2>&1
    }
    $results['gitingest'] = @{
        command   = "npx --yes gitingest $Repo"
        samplesMs = $r.samplesMs
        medianMs  = $r.medianMs
        note      = Summarize-Output $r.lastOutput
        exitCode  = $r.exitCode
    }
}

$freeGb = [math]::Round((Get-PSDrive C).Free / 1GB, 2)

$doc = [ordered]@{
    schemaVersion = 1
    measuredAt    = (Get-Date).ToUniversalTime().ToString('o')
    sourceRevision = (git rev-parse HEAD 2>&1 | Out-String).Trim()
    environment = [ordered]@{
        platform = 'win32-x64'
        node     = (node --version 2>&1 | Out-String).Trim()
        freeDiskGbStart = 19.18
        freeDiskGbEnd   = $freeGb
        cargoIncremental = 0
        buildProfile = 'release fat-LTO'
    }
    caveat = 'Tools measure different things; wall-clock only on this repo working tree.'
    method = [ordered]@{
        samplesPerTool = 3
        npxWarmupDiscarded = 1
        targetExcludedWhereSupported = $true
    }
    compile = [ordered]@{
        releaseBin = 'success'
        architectureSelfTest = '4 passed'
    }
    artifacts = $artifacts
    timings = $results
}

$outPath = Join-Path $Repo 'benchmark-results\competitor-round-2026-09-17.json'
$doc | ConvertTo-Json -Depth 12 | Set-Content -Path $outPath -Encoding utf8
Write-Host "Wrote $outPath"
Write-Host "FreeGB end: $freeGb"
