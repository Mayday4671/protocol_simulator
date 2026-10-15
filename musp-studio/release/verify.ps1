[CmdletBinding(DefaultParameterSetName = 'Manifest')]
param(
    [Parameter(ParameterSetName = 'Manifest')]
    [string] $ManifestPath,

    [Parameter(ParameterSetName = 'File', Mandatory = $true)]
    [string] $FilePath,

    [Parameter(ParameterSetName = 'File', Mandatory = $true)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedSha256
)

# Read-only verification. Never installs, launches, rewrites, or removes files.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-Checksum {
    param([string] $LiteralFile, [string] $Expected, [string] $Label)
    if (-not (Test-Path -LiteralPath $LiteralFile -PathType Leaf)) {
        Write-Host "MISSING  $Label"
        return $false
    }
    $actual = (Get-FileHash -LiteralPath $LiteralFile -Algorithm SHA256).Hash
    if ($actual -ine $Expected) {
        Write-Host "MISMATCH $Label"
        Write-Host "  expected: $Expected"
        Write-Host "  actual:   $actual"
        return $false
    }
    Write-Host "OK       $Label"
    return $true
}

try {
    if ($PSCmdlet.ParameterSetName -eq 'File') {
        if (Test-Checksum -LiteralFile $FilePath -Expected $ExpectedSha256 -Label $FilePath) { exit 0 }
        exit 1
    }

    if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
        $ManifestPath = Join-Path $PSScriptRoot 'SHA256SUMS.txt'
    }
    $manifest = Get-Item -LiteralPath $ManifestPath
    if ($manifest.PSIsContainer) { throw 'ManifestPath must name a file.' }
    $baseDirectory = $manifest.Directory.FullName
    $prefix = $baseDirectory.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $entries = [Collections.Generic.List[object]]::new()
    $lineNumber = 0

    # Validate the entire manifest before reading any referenced file.
    foreach ($line in (Get-Content -LiteralPath $manifest.FullName -Encoding UTF8)) {
        $lineNumber++
        if ([string]::IsNullOrWhiteSpace($line) -or $line.TrimStart().StartsWith('#')) { continue }
        if ($line -notmatch '^([0-9a-fA-F]{64})[ \t]+\*?(.+)$') {
            throw "Invalid SHA256 manifest line $lineNumber."
        }
        $expected = $Matches[1]
        $relativeName = $Matches[2]
        if ([IO.Path]::IsPathRooted($relativeName) -or $relativeName.Contains(':') -or
            ($relativeName -split '[/\\]') -contains '..') {
            throw "Unsafe manifest path on line $lineNumber."
        }
        $fullPath = [IO.Path]::GetFullPath((Join-Path $baseDirectory $relativeName))
        if (-not $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Manifest path escapes its directory on line $lineNumber."
        }
        if (-not $seen.Add($fullPath)) { throw "Duplicate manifest file on line $lineNumber." }

        # Do not follow junctions or symlinks outside the release directory.
        $cursor = $fullPath
        while ($cursor -and $cursor -ine $baseDirectory) {
            if (Test-Path -LiteralPath $cursor) {
                $item = Get-Item -LiteralPath $cursor -Force
                if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                    throw "Manifest path contains a reparse point on line $lineNumber."
                }
            }
            $cursor = [IO.Path]::GetDirectoryName($cursor)
        }
        $entries.Add([pscustomobject]@{ Path = $fullPath; Expected = $expected; Label = $relativeName })
    }
    if ($entries.Count -eq 0) { throw 'Manifest contains no checksum entries.' }
    $failed = 0
    foreach ($entry in $entries) {
        if (-not (Test-Checksum -LiteralFile $entry.Path -Expected $entry.Expected -Label $entry.Label)) { $failed++ }
    }
    Write-Host ("Verified {0} file(s); {1} failure(s)." -f $entries.Count, $failed)
    if ($failed -gt 0) { exit 1 }
    exit 0
}
catch {
    Write-Error $_ -ErrorAction Continue
    exit 2
}
