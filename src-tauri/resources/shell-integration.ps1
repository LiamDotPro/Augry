# Terminal Grid shell integration for PowerShell (pwsh and Windows PowerShell).
# Emits OSC 7 (cwd), OSC 133 (prompt/command markers) and OSC 7777 (last command) so the app can
# track the working directory, refresh git labels and detect finished agent runs.

[Console]::OutputEncoding = [Text.Encoding]::UTF8
$OutputEncoding = [Text.Encoding]::UTF8

if (Test-Path $PROFILE) { . $PROFILE }

$global:__tg_original_prompt = $function:prompt
$global:__tg_last_history_id = -1
$global:__tg_last_error = $null

function global:prompt {
    # Read $? and $LASTEXITCODE first: any other statement changes them.
    $ok = $?
    $native = $global:LASTEXITCODE
    # A failed native command sets $LASTEXITCODE and adds no error record. A
    # failed cmdlet adds one, and then a stale $LASTEXITCODE must not be reported.
    $lastError = if ($global:Error.Count -gt 0) { $global:Error[0] } else { $null }
    $newError = -not [object]::ReferenceEquals($lastError, $global:__tg_last_error)
    $global:__tg_last_error = $lastError
    $exit = if ($ok) { 0 } elseif (-not $newError -and $native -is [int] -and $native -ne 0) { $native } else { 1 }
    $esc = [char]27; $bel = [char]7

    $h = Get-History -Count 1
    # The -Command line that loaded this script is history entry 1; it is not
    # something the user ran, so it is never reported as the last command.
    if ($h -and $h.Id -ne $global:__tg_last_history_id -and $h.CommandLine -notlike '*shell-integration.ps1*') {
        $global:__tg_last_history_id = $h.Id
        $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($h.CommandLine))
        Write-Host -NoNewline "$esc]7777;cmd;$b64$bel"
        Write-Host -NoNewline "$esc]133;D;$exit$bel"
    }

    $p = (Get-Location).ProviderPath -replace '\\','/'
    Write-Host -NoNewline "$esc]7;file://localhost/$p$bel"
    Write-Host -NoNewline "$esc]133;A$bel"

    $out = if ($global:__tg_original_prompt) { & $global:__tg_original_prompt } else { "PS $($executionContext.SessionState.Path.CurrentLocation)> " }
    Write-Host -NoNewline "$esc]133;B$bel"
    return $out
}
