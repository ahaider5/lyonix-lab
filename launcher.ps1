[CmdletBinding()]
param(
    [string]$ModelsPath,
    [string]$ProfilesPath,
    [string]$IniPath,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$BaseDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
if (-not $ModelsPath) { $ModelsPath = Join-Path $BaseDir 'models.json' }
if (-not $ProfilesPath) { $ProfilesPath = Join-Path $BaseDir 'profiles.json' }
if (-not $IniPath) { $IniPath = Join-Path $BaseDir 'launcher.ini' }

Import-Module (Join-Path $BaseDir 'hardware.psm1') -Force
Import-Module (Join-Path $BaseDir 'recommendations.psm1') -Force

function Read-Ini {
    param([string]$Path)

    $data = [ordered]@{}
    if (-not (Test-Path -LiteralPath $Path)) { return $data }

    $section = 'launcher'
    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = $line.Trim()
        if ($trimmed -eq '' -or $trimmed.StartsWith(';') -or $trimmed.StartsWith('#')) { continue }
        if ($trimmed -match '^\[(.+)\]$') {
            $section = $matches[1]
            if (-not $data.Contains($section)) { $data[$section] = [ordered]@{} }
            continue
        }
        if ($trimmed -match '^([^=]+)=(.*)$') {
            if (-not $data.Contains($section)) { $data[$section] = [ordered]@{} }
            $data[$section][$matches[1].Trim()] = $matches[2].Trim()
        }
    }

    return , $data
}

function Write-Ini {
    param([string]$Path, [System.Collections.IDictionary]$Data)

    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($section in $Data.Keys) {
        $lines.Add("[$section]")
        foreach ($key in $Data[$section].Keys) { $lines.Add("$key=$($Data[$section][$key])") }
        $lines.Add('')
    }
    Set-Content -LiteralPath $Path -Value $lines -Encoding ASCII
}

function Get-IniValue {
    param($Ini, [string]$Section, [string]$Name, [string]$Default = '')
    if ($Ini.Contains($Section) -and $Ini[$Section].Contains($Name)) { return $Ini[$Section][$Name] }
    return $Default
}

function Set-IniValue {
    param([System.Collections.IDictionary]$Ini, [string]$Section, [string]$Name, [string]$Value)
    if (-not $Ini.Contains($Section)) { $Ini[$Section] = [ordered]@{} }
    $Ini[$Section][$Name] = $Value
}

function Normalize-LauncherInput {
    param([string]$Value)
    if ($null -eq $Value) { return '' }
    return $Value.Replace('???', '').Trim([char]0xFEFF).Trim()
}

function Expand-EnvironmentVariables {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return $Value }
    return [Environment]::ExpandEnvironmentVariables($Value)
}

function Get-PathRoots {
    param([string]$ModelsRoot, [string]$BaseDir)

    $roots = New-Object System.Collections.Generic.List[string]
    $envRoot = $env:LYONIX_MODELS_ROOT
    $candidates = @($ModelsRoot, $envRoot, (Join-Path $BaseDir 'models'))

    foreach ($candidate in $candidates) {
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        foreach ($root in ($candidate -split ';')) {
            if ([string]::IsNullOrWhiteSpace($root)) { continue }
            $expanded = Expand-EnvironmentVariables -Value $root.Trim()
            if (-not [System.IO.Path]::IsPathRooted($expanded)) {
                $expanded = Join-Path $BaseDir $expanded
            }
            $full = [System.IO.Path]::GetFullPath($expanded)
            if (-not ($roots -contains $full)) { $roots.Add($full) }
        }
    }

    return ,$roots.ToArray()
}

function Resolve-ConfiguredPath {
    param([string]$Path, [string[]]$Roots)

    if ([string]::IsNullOrWhiteSpace($Path)) { return '' }
    $expanded = Expand-EnvironmentVariables -Value $Path.Trim()

    if ([System.IO.Path]::IsPathRooted($expanded)) {
        return [System.IO.Path]::GetFullPath($expanded)
    }

    foreach ($root in $Roots) {
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $root $expanded))
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }

    if ($Roots.Count -gt 0) {
        return [System.IO.Path]::GetFullPath((Join-Path $Roots[0] $expanded))
    }

    return [System.IO.Path]::GetFullPath((Join-Path $BaseDir $expanded))
}

function Resolve-LlamaServerExecutable {
    param([string]$ConfiguredPath)

    $expanded = Expand-EnvironmentVariables -Value $ConfiguredPath
    if ([string]::IsNullOrWhiteSpace($expanded)) { return 'llama-server' }

    if (Test-Path -LiteralPath $expanded) {
        return [System.IO.Path]::GetFullPath($expanded)
    }

    try {
        return (Get-Command $expanded -ErrorAction Stop).Source
    } catch {
        return $expanded
    }
}

function Merge-Ini {
    param($Base, $Overlay)

    if ($null -eq $Overlay) { return $Base }
    foreach ($section in $Overlay.Keys) {
        if (-not $Base.Contains($section)) { $Base[$section] = [ordered]@{} }
        foreach ($key in $Overlay[$section].Keys) { $Base[$section][$key] = $Overlay[$section][$key] }
    }
    return $Base
}

function Select-ItemFromMenu {
    param([Parameter(Mandatory)][array]$Items, [Parameter(Mandatory)][string]$Title, [string]$DefaultId)

    Write-Host ''
    Write-Host $Title
    Write-Host ('-' * $Title.Length)
    for ($i = 0; $i -lt $Items.Count; $i++) {
        $mark = if ($Items[$i].id -eq $DefaultId) { '*' } else { ' ' }
        Write-Host ("{0,2}. {1} {2}" -f ($i + 1), $Items[$i].displayName, $mark)
    }

    while ($true) {
        $answer = (Normalize-LauncherInput (Read-Host "Choose 1-$($Items.Count) or press Enter for default")) -replace '^\D+(?=\d)', ''
        if ([string]::IsNullOrWhiteSpace($answer) -and $DefaultId) {
            $default = $Items | Where-Object { $_.id -eq $DefaultId } | Select-Object -First 1
            if ($default) { return $default }
        }
        $number = 0
        if ([int]::TryParse($answer, [ref]$number) -and $number -ge 1 -and $number -le $Items.Count) { return $Items[$number - 1] }
        Write-Warning 'Invalid selection.'
    }
}

function Read-Override {
    param([string]$Name, $Current)

    $answer = Normalize-LauncherInput (Read-Host "$Name [$Current]")
    if ([string]::IsNullOrWhiteSpace($answer)) { return $Current }
    if ($Current -is [bool]) { return $answer -match '^(1|true|yes|y|on)$' }
    if ($Current -is [int]) { return [int]$answer }
    if ($Current -is [double]) { return [double]$answer }
    return $answer
}

function Quote-Argument {
    param([string]$Value)

    if ($Value -match '[\s"]') { return '"' + ($Value -replace '"', '\"') + '"' }
    return $Value
}

function Split-CommandLine {
    param([string]$CommandLine)

    if ([string]::IsNullOrWhiteSpace($CommandLine)) { return @() }
    $matches = [regex]::Matches($CommandLine, '("([^"\\]|\\.)*"|''[^'']*''|\S+)')
    foreach ($match in $matches) {
        $value = $match.Value
        if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        $value -replace '\\"', '"'
    }
}

function New-LaunchArguments {
    param($Settings, $Model, $LauncherSettings, [string[]]$ExtraArgs)

    $hostName = Get-IniValue -Ini $LauncherSettings -Section 'launcher' -Name 'host' -Default '127.0.0.1'
    $port = Get-IniValue -Ini $LauncherSettings -Section 'launcher' -Name 'port' -Default '8081'

    $args = New-Object System.Collections.Generic.List[string]
    $args.Add('-m'); $args.Add($Model.ResolvedPath)
    if ($Model.ResolvedMmprojPath) { $args.Add('--mmproj'); $args.Add($Model.ResolvedMmprojPath) }
    $args.Add('-c'); $args.Add([string]$Settings.ContextSize)
    $args.Add('-ngl'); $args.Add([string]$Settings.GpuLayers)
    $args.Add('-t'); $args.Add([string]$Settings.Threads)
    $args.Add('-b'); $args.Add([string]$Settings.BatchSize)
    $args.Add('-ub'); $args.Add([string]$Settings.UbatchSize)
    $args.Add('--parallel'); $args.Add([string]$Settings.Parallel)
    $args.Add('--host'); $args.Add($hostName)
    $args.Add('--port'); $args.Add($port)
    if ($Settings.Fit) {
        $args.Add('--fit'); $args.Add('on')
        if ($Settings.FitTargetMiB -gt 0) { $args.Add('--fit-target'); $args.Add([string]$Settings.FitTargetMiB) }
    } else {
        $args.Add('--fit'); $args.Add('off')
    }
    if ($Settings.Device) { $args.Add('--device'); $args.Add($Settings.Device) }
    $args.Add('--flash-attn'); $args.Add([string]$Settings.FlashAttention)
    $args.Add('--cache-type-k'); $args.Add([string]$Settings.CacheTypeK)
    $args.Add('--cache-type-v'); $args.Add([string]$Settings.CacheTypeV)
    $args.Add('--cache-ram'); $args.Add([string]$Settings.CacheRamMiB)
    $args.Add('--cache-reuse'); $args.Add([string]$Settings.CacheReuse)
    $args.Add('--temp'); $args.Add([string]$Settings.Temperature)
    $args.Add('--top-p'); $args.Add([string]$Settings.TopP)
    $args.Add('--top-k'); $args.Add([string]$Settings.TopK)
    $args.Add('--min-p'); $args.Add([string]$Settings.MinP)
    $args.Add('--repeat-penalty'); $args.Add([string]$Settings.RepeatPenalty)
    $args.Add('--presence-penalty'); $args.Add([string]$Settings.PresencePenalty)
    if ($Settings.Jinja) { $args.Add('--jinja') }
    if ($Settings.Reasoning -in @('on','off','auto')) { $args.Add('--reasoning'); $args.Add([string]$Settings.Reasoning) }
    if ($Settings.ReasoningFormat) { $args.Add('--reasoning-format'); $args.Add([string]$Settings.ReasoningFormat) }
    if ($Model.reasoning -and $Settings.ReasoningBudget -ge 0) {
        $args.Add('--reasoning-budget'); $args.Add([string]$Settings.ReasoningBudget)
    }
    # Built-in llama-server host tools remain disabled by default.
    # External agent frameworks such as Hermes own their tool boundary.
    if ($Settings.Numa) { $args.Add('--numa'); $args.Add('distribute') }
    if ($Settings.Mlock) { $args.Add('--mlock') }
    if ($Settings.NoMmap) { $args.Add('--no-mmap') }

    foreach ($arg in $ExtraArgs) {
        foreach ($part in (Split-CommandLine -CommandLine $arg)) {
            if (-not [string]::IsNullOrWhiteSpace($part)) { $args.Add($part) }
        }
    }

    return , $args.ToArray()
}

function New-LaunchCommand {
    param([string]$Executable, [string[]]$Arguments)

    $parts = @((Quote-Argument $Executable)) + ($Arguments | ForEach-Object { Quote-Argument $_ })
    return ($parts -join ' ')
}

function Show-Recommendation {
    param($Hardware, $Model, $Profile, $Settings)

    Write-Host ''
    Write-Host 'Recommended Configuration'
    Write-Host '-------------------------'
    Write-Host "Model : $($Model.displayName) ($($Model.quantization), $($Model.parametersB)B)"
    Write-Host "Profile : $($Profile.displayName)"
    Write-Host "Hardware Tier : $($Settings.HardwareTier)"
    Write-Host "GPU : $($Hardware.GpuModel) ($($Hardware.DedicatedVramGB) GB)"
    Write-Host "Llama Device : $(if ($Settings.Device) { $Settings.Device } else { 'CPU/default backend' })"
    Write-Host "RAM : $($Hardware.InstalledRamGB) GB installed, $($Hardware.AvailableRamGB) GB available"
    Write-Host "CPU : $($Hardware.CpuModel) ($($Hardware.LogicalCores) Threads)"
    Write-Host "OS  : $($Hardware.OperatingSystem)"
    Write-Host ''
    Write-Host "Context Size : $($Settings.ContextSize)"
    Write-Host "Reason : $($Settings.Reasons.ContextSize)"
    Write-Host "GPU Layers : $($Settings.GpuLayers)"
    Write-Host "Reason : $($Settings.Reasons.GpuLayers)"
    Write-Host "Fit Target : $($Settings.FitTargetMiB) MiB"
    Write-Host "Reason : $($Settings.Reasons.FitTarget)"
    Write-Host "Threads : $($Settings.Threads)"
    Write-Host "Reason : $($Settings.Reasons.Threads)"
    Write-Host "Batch / UBatch : $($Settings.BatchSize) / $($Settings.UbatchSize)"
    Write-Host "Reason : $($Settings.Reasons.BatchSize)"
    Write-Host "Parallel Requests : $($Settings.Parallel)"
    Write-Host "Reason : $($Settings.Reasons.Parallel)"
    Write-Host "Flash Attention : $($Settings.FlashAttention)"
    Write-Host "Reason : $($Settings.Reasons.FlashAttention)"
    Write-Host "GPU Device Reason : $($Settings.Reasons.GpuDevice)"
    Write-Host "KV Cache K/V : $($Settings.CacheTypeK) / $($Settings.CacheTypeV)"
    Write-Host "Prompt Cache RAM : $($Settings.CacheRamMiB) MiB"
    Write-Host "Reason : $($Settings.Reasons.Cache)"
    Write-Host "Reasoning : $($Settings.Reasoning)$(if ($Settings.ReasoningBudget -gt 0) { " (budget $($Settings.ReasoningBudget))" })"
    Write-Host "Reason : $($Settings.Reasons.Reasoning)"
    Write-Host ''
    Write-Host "Estimated RAM Usage : $($Settings.Estimated.EstimatedRamGB) GB (advisory)"
    Write-Host "Estimated Full GPU VRAM Usage : $($Settings.Estimated.EstimatedFullGpuVramGB) GB (advisory; actual offload is handled by llama.cpp)"
    if ($Settings.RecommendedDefaultModel) {
        Write-Host "Model Recommendation : $($Settings.RecommendedDefaultModel)"
    }
}

function Test-LaunchValidation {
    param($Model, $Hardware, $Settings, [string[]]$ExtraArgs)

    $warnings = New-Object System.Collections.Generic.List[string]
    if (-not (Test-Path -LiteralPath $Model.ResolvedPath)) { $warnings.Add("Model file does not exist: $($Model.ResolvedPath)") }
    if ($Model.vision -and -not $Model.ResolvedMmprojPath) { $warnings.Add('Selected model is marked as vision-capable but no mmproj path is configured.') }
    if ($Model.ResolvedMmprojPath -and -not (Test-Path -LiteralPath $Model.ResolvedMmprojPath)) { $warnings.Add("mmproj file does not exist: $($Model.ResolvedMmprojPath)") }
    if ($Settings.ContextSize -gt [int]$Model.contextLimit) { $warnings.Add("Requested context $($Settings.ContextSize) exceeds model context limit $($Model.contextLimit).") }
    if ($Hardware.AvailableRamGB -gt 0 -and $Hardware.AvailableRamGB -lt 2) { $warnings.Add('Currently available system RAM is below 2 GB. Close unused applications before starting the model to avoid paging.') }
    if ($Settings.CacheRamMiB -gt 0 -and $Hardware.InstalledRamGB -le 16 -and $Settings.CacheRamMiB -gt 2048) { $warnings.Add('Prompt-cache ceiling is large for a <=16 GB system; consider reducing it if Windows starts paging.') }
    if (-not $Settings.Device -and $Settings.GpuLayers -ne '0' -and $Hardware.IsDedicatedGpu) { $warnings.Add('No llama.cpp device name was detected for the dedicated GPU; llama.cpp will choose its default device.') }

    $managedPatterns = @(
        '(^|\s)--?(?:ctx-size|c)(\s|=)',
        '(^|\s)--?(?:n-gpu-layers|gpu-layers|ngl)(\s|=)',
        '(^|\s)--?(?:device|dev)(\s|=)',
        '(^|\s)--?(?:port)(\s|=)',
        '(^|\s)--?(?:host)(\s|=)',
        '(^|\s)--?(?:threads|t)(\s|=)',
        '(^|\s)--?(?:batch-size|b)(\s|=)',
        '(^|\s)--?(?:ubatch-size|ub)(\s|=)',
        '(^|\s)--?(?:flash-attn|fa)(\s|=)',
        '(^|\s)--?(?:cache-type-k|ctk)(\s|=)',
        '(^|\s)--?(?:cache-type-v|ctv)(\s|=)',
        '(^|\s)--?(?:cache-ram|cram)(\s|=)',
        '(^|\s)--?(?:reasoning|reasoning-budget)(\s|=)'
    )
    if ($ExtraArgs -and $ExtraArgs.Count -gt 0) {
        $combinedExtra = ($ExtraArgs -join ' ')
        foreach ($pattern in $managedPatterns) {
            if ($combinedExtra -match $pattern) {
                $warnings.Add('Custom command-line arguments contain a launcher-managed option. Remove it from customArgs so the hardware-aware recommendation remains authoritative.')
                break
            }
        }
    }

    return , $warnings
}

function Read-JsonArray {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) { throw "Required configuration file not found: $Path" }
    $items = ConvertFrom-Json -InputObject (Get-Content -Raw -LiteralPath $Path)
    return @($items)
}

$models = Read-JsonArray -Path $ModelsPath
$profiles = Read-JsonArray -Path $ProfilesPath
$ini = Read-Ini -Path $IniPath

# Optional per-machine overlay. This file is intended to be gitignored.
$userIniPath = Join-Path $BaseDir 'launcher.user.ini'
if (Test-Path -LiteralPath $userIniPath) {
    $userIni = Read-Ini -Path $userIniPath
    $ini = Merge-Ini -Base $ini -Overlay $userIni
}

$llamaServerConfigured = Get-IniValue -Ini $ini -Section 'launcher' -Name 'llamaServer' -Default 'llama-server'
$llamaServer = Resolve-LlamaServerExecutable -ConfiguredPath $llamaServerConfigured
$modelsRoot = Get-IniValue -Ini $ini -Section 'launcher' -Name 'modelsRoot' -Default 'models'
$pathRoots = Get-PathRoots -ModelsRoot $modelsRoot -BaseDir $BaseDir

foreach ($entry in $models) {
    $entry | Add-Member -NotePropertyName ResolvedPath -NotePropertyValue (Resolve-ConfiguredPath -Path $entry.path -Roots $pathRoots) -Force
    if ($entry.mmprojPath) {
        $entry | Add-Member -NotePropertyName ResolvedMmprojPath -NotePropertyValue (Resolve-ConfiguredPath -Path $entry.mmprojPath -Roots $pathRoots) -Force
    } else {
        $entry | Add-Member -NotePropertyName ResolvedMmprojPath -NotePropertyValue '' -Force
    }
}

$lastModel = Get-IniValue -Ini $ini -Section 'launcher' -Name 'lastModel'
$lastProfile = Get-IniValue -Ini $ini -Section 'launcher' -Name 'lastProfile'
$hardware = Get-LauncherHardware -LlamaServer $llamaServer
$preferredDevice = Select-PreferredLlamaDevice -Hardware $hardware
$recommendedModelId = Get-RecommendedModelId -Hardware $hardware -Models $models

# Prefer a previous selection when that model still exists. Otherwise choose the
# hardware-aware model recommendation, or the first model whose file exists.
$defaultModelId = $lastModel
$lastModelObj = $models | Where-Object { $_.id -eq $lastModel } | Select-Object -First 1
if (-not $lastModelObj -or -not (Test-Path -LiteralPath $lastModelObj.ResolvedPath)) {
    $recommendedObj = $models | Where-Object { $_.id -eq $recommendedModelId } | Select-Object -First 1
    if ($recommendedObj -and (Test-Path -LiteralPath $recommendedObj.ResolvedPath)) {
        $defaultModelId = $recommendedObj.id
    } else {
        $availableModel = $models | Where-Object { Test-Path -LiteralPath $_.ResolvedPath } | Select-Object -First 1
        $defaultModelId = if ($availableModel) { $availableModel.id } else { $recommendedModelId }
    }
}

$model = Select-ItemFromMenu -Items $models -Title 'Select Model' -DefaultId $defaultModelId
$defaultProfileId = if ($lastProfile) { $lastProfile } elseif ($model.defaultProfile) { $model.defaultProfile } else { 'chat' }
$profile = Select-ItemFromMenu -Items $profiles -Title 'Select Profile' -DefaultId $defaultProfileId
$settings = Get-LaunchRecommendation -Hardware $hardware -Model $model -Profile $profile

if ($preferredDevice -and $settings.GpuLayers -ne '0') {
    $settings | Add-Member -NotePropertyName Device -NotePropertyValue $preferredDevice.Id -Force
    $settings.Reasons.GpuDevice = "Selected $($preferredDevice.Id) ($($preferredDevice.Name)) because it matches the best detected dedicated GPU."
} else {
    $fallbackDevice = if ($settings.GpuLayers -eq '0') { 'none' } else { '' }
    $settings | Add-Member -NotePropertyName Device -NotePropertyValue $fallbackDevice -Force
    $settings.Reasons.GpuDevice = if ($settings.GpuLayers -eq '0') { 'GPU offload is disabled.' } else { 'No matching llama.cpp GPU device was detected.' }
}
$settings | Add-Member -NotePropertyName RecommendedDefaultModel -NotePropertyValue $recommendedModelId -Force

Show-Recommendation -Hardware $hardware -Model $model -Profile $profile -Settings $settings

if ((Normalize-LauncherInput (Read-Host 'Override recommended values? [y/N]')) -match '^(y|yes)$') {
    $settings.ContextSize = [int](Read-Override -Name 'Context Size' -Current $settings.ContextSize)
    $settings.GpuLayers = Read-Override -Name 'GPU Layers (auto/all/0/or number)' -Current $settings.GpuLayers
    $settings.Threads = [int](Read-Override -Name 'Threads' -Current $settings.Threads)
    $settings.BatchSize = [int](Read-Override -Name 'Batch Size' -Current $settings.BatchSize)
    $settings.UbatchSize = [int](Read-Override -Name 'UBatch Size' -Current $settings.UbatchSize)
    $settings.Parallel = [int](Read-Override -Name 'Parallel Requests' -Current $settings.Parallel)
    $settings.FlashAttention = (Read-Override -Name 'Flash Attention (auto/on/off)' -Current $settings.FlashAttention).ToLowerInvariant()
    $settings.CacheTypeK = (Read-Override -Name 'KV Cache K' -Current $settings.CacheTypeK).ToLowerInvariant()
    $settings.CacheTypeV = (Read-Override -Name 'KV Cache V' -Current $settings.CacheTypeV).ToLowerInvariant()
    $settings.CacheRamMiB = [int](Read-Override -Name 'Prompt Cache RAM MiB' -Current $settings.CacheRamMiB)
    $settings.Temperature = [double](Read-Override -Name 'Temperature' -Current $settings.Temperature)
    $settings.TopP = [double](Read-Override -Name 'Top P' -Current $settings.TopP)
    $settings.TopK = [int](Read-Override -Name 'Top K' -Current $settings.TopK)
    $settings.MinP = [double](Read-Override -Name 'Min P' -Current $settings.MinP)
    $settings.RepeatPenalty = [double](Read-Override -Name 'Repeat Penalty' -Current $settings.RepeatPenalty)
    $settings.PresencePenalty = [double](Read-Override -Name 'Presence Penalty' -Current $settings.PresencePenalty)
    $settings.Reasoning = (Read-Override -Name 'Reasoning (on/off/auto)' -Current $settings.Reasoning).ToLowerInvariant()
    if ($model.reasoning) {
        $settings.ReasoningBudget = [int](Read-Override -Name 'Reasoning Budget' -Current $settings.ReasoningBudget)
    }
    $settings.Estimated = Get-ModelEstimatedMemory -Model $model -ContextSize $settings.ContextSize
}

$customArgs = Get-IniValue -Ini $ini -Section 'launcher' -Name 'customArgs'
$extraInput = Normalize-LauncherInput (Read-Host "Custom command-line arguments [$customArgs]")
if (-not [string]::IsNullOrWhiteSpace($extraInput)) { $customArgs = $extraInput }

$extraArgs = @()
if ($profile.extraArgs) { $extraArgs += @($profile.extraArgs) }
if (-not [string]::IsNullOrWhiteSpace($customArgs)) { $extraArgs += $customArgs }

$warnings = Test-LaunchValidation -Model $model -Hardware $hardware -Settings $settings -ExtraArgs $extraArgs

if ($settings.GpuLayers -eq 'auto' -and $hardware.DedicatedVramGB -gt 0) {
    # Leave the fit logic to llama.cpp. Only the device and safety margin are selected here.
    $settings.GpuLayers = 'auto'
}

if ($warnings.Count -gt 0) {
    Write-Host ''
    Write-Warning 'Validation warnings:'
    foreach ($warning in $warnings | Select-Object -Unique) { Write-Warning $warning }
}

$launchArgs = New-LaunchArguments -Settings $settings -Model $model -LauncherSettings $ini -ExtraArgs $extraArgs
$command = New-LaunchCommand -Executable $llamaServer -Arguments $launchArgs

Write-Host ''
Write-Host 'Final Command'
Write-Host '-------------'
Write-Host $command

$exportPath = Get-IniValue -Ini $ini -Section 'launcher' -Name 'exportCommandPath' -Default 'last-launch-command.txt'
if (-not [System.IO.Path]::IsPathRooted($exportPath)) { $exportPath = Join-Path $BaseDir $exportPath }
Set-Content -LiteralPath $exportPath -Value $command -Encoding ASCII
Write-Host "Exported command to: $exportPath"

# Persist user selections to the local overlay rather than changing the shareable
# launcher.ini. This keeps machine-specific state out of version control.
$persistPath = $userIniPath
$persist = if (Test-Path -LiteralPath $persistPath) { Read-Ini -Path $persistPath } else { [ordered]@{} }
Set-IniValue -Ini $persist -Section 'launcher' -Name 'lastModel' -Value $model.id
Set-IniValue -Ini $persist -Section 'launcher' -Name 'lastProfile' -Value $profile.id
Set-IniValue -Ini $persist -Section 'launcher' -Name 'modelsRoot' -Value $modelsRoot
Set-IniValue -Ini $persist -Section 'launcher' -Name 'customArgs' -Value $customArgs
Write-Ini -Path $persistPath -Data $persist

if ($DryRun) {
    Write-Host 'Dry run selected; llama-server was not started.'
    exit 0
}

if (-not (Get-Command $llamaServer -ErrorAction SilentlyContinue) -and -not (Test-Path -LiteralPath $llamaServer)) {
    throw "llama-server executable was not found: $llamaServer"
}

$existing = @(Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue)
$preventDuplicate = (Get-IniValue -Ini $ini -Section 'launcher' -Name 'preventDuplicate' -Default 'true') -match '^(true|1|yes)$'
$stopExisting = (Get-IniValue -Ini $ini -Section 'launcher' -Name 'stopExisting' -Default 'false') -match '^(true|1|yes)$'

if ($existing.Count -gt 0) {
    if ($stopExisting) {
        $existing | Stop-Process -Force
        Write-Host "Stopped $($existing.Count) existing llama-server process(es)."
    }
    elseif ($preventDuplicate) {
        Write-Warning 'A llama-server process is already running. Set stopExisting=true or preventDuplicate=false in launcher.ini/user overlay to change this behavior.'
        exit 1
    }
}

$startMinimized = (Get-IniValue -Ini $ini -Section 'launcher' -Name 'startMinimized' -Default 'true') -match '^(true|1|yes)$'
$windowStyle = if ($startMinimized) { 'Minimized' } else { 'Normal' }
Start-Process -FilePath $llamaServer -ArgumentList $launchArgs -WindowStyle $windowStyle
