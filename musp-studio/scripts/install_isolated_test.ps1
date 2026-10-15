param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$testRoot = Join-Path $workspace 'test-results\flow-workspace-install'
$installTarget = [IO.Path]::GetFullPath((Join-Path $testRoot 'installed'))
if (-not $installTarget.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installation target must stay in the workspace' }
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$existingRegistration = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MUSP Studio'
$existingBinary = Join-Path $existingRegistration.InstallLocation.Trim('"') 'musp-studio.exe'
$existingHash = (Get-FileHash -LiteralPath $existingBinary -Algorithm SHA256).Hash
$runningBefore = @(Get-Process 'musp-studio' -ErrorAction SilentlyContinue | Select-Object Id, Path)
if ($runningBefore | Where-Object { $_.Path -eq (Join-Path $installTarget 'musp-studio.exe') }) { throw 'The isolated installation is currently running' }
# /NS suppresses shortcut creation. Preserve the two application registration keys
# so a side-by-side install test cannot redirect the user's actual installation.
$keys = @('HKCU\Software\musp\MUSP Studio', 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\MUSP Studio')
$backups = @()
foreach ($key in $keys) {
    $backup = Join-Path $testRoot ("registry-{0}.reg" -f $backups.Count)
    & reg.exe export $key $backup /y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Cannot back up existing app registration: $key" }
    $backups += $backup
}
$report = @{passed=$false; installer=$installerPath; installTarget=$installTarget; userProcesses=$runningBefore; startedAt=(Get-Date).ToString('o')}
try {
    $process = Start-Process -FilePath $installerPath -ArgumentList @('/S','/NS',('/D=' + $installTarget)) -WindowStyle Hidden -PassThru
    $process.WaitForExit()
    $report.exitCode = $process.ExitCode
    if ($process.ExitCode -ne 0) { throw "NSIS exit $($process.ExitCode)" }
    $binary = Join-Path $installTarget 'musp-studio.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw 'Installed executable missing' }
    $report.installerSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash
    $report.installedSha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
    if ((Get-FileHash -LiteralPath $existingBinary -Algorithm SHA256).Hash -ne $existingHash) { throw 'Existing installation binary changed' }
    $report.existingInstallationUnchanged = $true
    foreach ($processBefore in $runningBefore) {
        $alive = Get-Process -Id $processBefore.Id -ErrorAction Stop
        if ($alive.Path -ne $processBefore.Path) { throw 'User process changed during installation' }
    }
    $report.passed = $true
} finally {
    foreach ($backup in $backups) {
        & reg.exe import $backup | Out-Null
        if ($LASTEXITCODE -ne 0) { $report.passed=$false; throw "Cannot restore application registration: $backup" }
    }
    $report.registrationRestored = $true
    $restored = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MUSP Studio'
    if ($restored.InstallLocation -ne $existingRegistration.InstallLocation -or $restored.DisplayVersion -ne $existingRegistration.DisplayVersion) { $report.passed=$false; throw 'Existing application registration differs after restoration' }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $testRoot 'report.json') -Encoding UTF8
}
$report | ConvertTo-Json -Depth 8
