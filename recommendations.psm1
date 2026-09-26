function Get-QuantBits {
    param([string]$Quantization)

    switch -Regex ($Quantization) {
        'Q2' { return 2.7 }
        'Q3' { return 3.5 }
        'Q4' { return 4.8 }
        'Q5' { return 5.8 }
        'Q6' { return 6.8 }
        'Q8' { return 8.5 }
        'F16|BF16' { return 16.0 }
        default { return 5.0 }
    }
}

function Get-ModelEstimatedMemory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Model,
        [int]$ContextSize = 8192
    )

    $fileSizeGB = 0
    if ($Model.ResolvedPath -and (Test-Path -LiteralPath $Model.ResolvedPath)) {
        $fileSizeGB = [math]::Round((Get-Item -LiteralPath $Model.ResolvedPath).Length / 1GB, 2)
    } elseif ($Model.path -and (Test-Path -LiteralPath $Model.path)) {
        $fileSizeGB = [math]::Round((Get-Item -LiteralPath $Model.path).Length / 1GB, 2)
    }

    if ($fileSizeGB -le 0) {
        $bits = Get-QuantBits -Quantization $Model.quantization
        $fileSizeGB = [math]::Round(($Model.parametersB * 1000000000 * ($bits / 8) * 1.12) / 1GB, 2)
    }

    # This is intentionally an advisory estimate. Prefer architecture-specific
    # KV metadata when available; exact allocator usage remains a llama.cpp concern.
    $kvBytesF16 = 0
    if ($Model.runtimeHints -and $Model.runtimeHints.kvCacheBytesPerTokenF16) {
        $kvBytesF16 = [double]$Model.runtimeHints.kvCacheBytesPerTokenF16
    } elseif ($Model.runtimeHints -and $Model.runtimeHints.fullAttentionLayers -and $Model.runtimeHints.kvHeads -and $Model.runtimeHints.headDim) {
        $kvBytesF16 = [double]$Model.runtimeHints.fullAttentionLayers * [double]$Model.runtimeHints.kvHeads * [double]$Model.runtimeHints.headDim * 2 * 2
    } else {
        $kvBytesF16 = [double]$Model.parametersB * 16384.0
    }
    # The default profile uses q8_0 KV, approximately half the f16 footprint.
    $kvCacheGB = [math]::Round(($kvBytesF16 * 0.5 * $ContextSize) / 1GB, 2)
    $visionReserveGB = if ($Model.vision) { 0.75 } else { 0.0 }
    $runtimeOverheadGB = [math]::Max(0.50, [math]::Round($fileSizeGB * 0.12, 2))
    $totalRamGB = [math]::Round($fileSizeGB + $kvCacheGB + $visionReserveGB + $runtimeOverheadGB, 2)
    $fullGpuGB = [math]::Round($fileSizeGB + ($kvCacheGB * 0.65) + $visionReserveGB + 0.50, 2)

    [pscustomobject]@{
        ModelFileGB = $fileSizeGB
        KvCacheGB = $kvCacheGB
        VisionReserveGB = $visionReserveGB
        RuntimeOverheadGB = $runtimeOverheadGB
        EstimatedRamGB = $totalRamGB
        EstimatedFullGpuVramGB = $fullGpuGB
    }
}

function Resolve-ProfileValue {
    param($Profile, [string]$Name, $Fallback)

    if ($Profile.PSObject.Properties.Name -contains $Name -and $null -ne $Profile.$Name) {
        return $Profile.$Name
    }

    return $Fallback
}

function Get-HardwareTier {
    param($Hardware)

    $ram = [double]$Hardware.InstalledRamGB
    $vram = [double]$Hardware.DedicatedVramGB
    $threads = [int]$Hardware.LogicalCores

    if ($ram -lt 8 -or $threads -le 4) { return 'entry' }
    if ($ram -lt 12) { return 'light-laptop' }
    if ($ram -lt 16 -or $vram -lt 2) { return 'laptop' }
    if ($ram -lt 32 -or $vram -lt 6) { return 'standard' }
    if ($ram -lt 64 -or $vram -lt 12) { return 'high' }
    return 'enthusiast'
}

function Get-ContextCeilingForHardware {
    param($Hardware, [int]$ModelContextLimit)

    $ram = [double]$Hardware.InstalledRamGB
    $vram = [double]$Hardware.DedicatedVramGB

    if ($ram -lt 8) { $ceiling = 4096 }
    elseif ($ram -lt 16) { $ceiling = 8192 }
    elseif ($ram -lt 24) { $ceiling = 16384 }
    elseif ($ram -lt 32) { $ceiling = 32768 }
    elseif ($ram -lt 64) { $ceiling = 32768 }
    else { $ceiling = 65536 }

    # A larger dedicated GPU can justify another context tier, but only if
    # system RAM is also substantial enough to keep the rest of the machine responsive.
    if ($vram -ge 12 -and $ram -ge 32) { $ceiling = [math]::Max($ceiling, 65536) }
    elseif ($vram -ge 8 -and $ram -ge 24) { $ceiling = [math]::Max($ceiling, 32768) }

    return [int][math]::Min($ceiling, [math]::Max(2048, $ModelContextLimit))
}

function Get-AdaptiveFitTargetMiB {
    param($Hardware)

    $vram = [double]$Hardware.DedicatedVramGB
    if ($vram -le 0) { return 0 }
    if ($vram -lt 3) { return 256 }
    if ($vram -lt 6) { return 512 }
    if ($vram -lt 12) { return 768 }
    return 1024
}

function Get-AdaptiveCacheRamMiB {
    param($Hardware)

    $ram = [double]$Hardware.InstalledRamGB
    if ($ram -lt 8) { return 512 }
    if ($ram -lt 16) { return 768 }
    if ($ram -lt 24) { return 1536 }
    if ($ram -lt 32) { return 2048 }
    if ($ram -lt 64) { return 4096 }
    return 8192
}

function Get-AdaptiveReasoningBudget {
    param($Hardware, [string]$ProfileId)

    if ($ProfileId -ne 'reasoning') { return 0 }

    $vram = [double]$Hardware.DedicatedVramGB
    $ram = [double]$Hardware.InstalledRamGB
    if ($vram -lt 3 -or $ram -lt 12) { return 512 }
    if ($vram -lt 6 -or $ram -lt 24) { return 1024 }
    if ($vram -lt 12 -or $ram -lt 32) { return 2048 }
    return 4096
}

function Get-LaunchRecommendation {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Hardware,
        [Parameter(Mandatory)]$Model,
        [Parameter(Mandatory)]$Profile
    )

    $hardwareTier = Get-HardwareTier -Hardware $Hardware
    $modelLimit = if ($Model.contextLimit) { [int]$Model.contextLimit } else { 8192 }
    $profileTargetContext = [int](Resolve-ProfileValue -Profile $Profile -Name 'targetContext' -Fallback 8192)
    $hardwareContextCeiling = Get-ContextCeilingForHardware -Hardware $Hardware -ModelContextLimit $modelLimit
    $modelRecommendedContext = [int](Resolve-ProfileValue -Profile $Model -Name 'recommendedContext' -Fallback $modelLimit)
    $contextSize = [int][math]::Min([math]::Min($profileTargetContext, $hardwareContextCeiling), [math]::Max(2048, $modelRecommendedContext))

    # Do not aggressively slash context merely because other Windows apps are open.
    # Only step down when free RAM is genuinely low enough that launch risk is high.
    $availableRam = [double]$Hardware.AvailableRamGB
    $memory = Get-ModelEstimatedMemory -Model $Model -ContextSize $contextSize
    while ($contextSize -gt 2048 -and $availableRam -gt 0 -and $availableRam -lt 1.5 -and $memory.EstimatedRamGB -gt [math]::Max(2, $availableRam + 1.0)) {
        $contextSize = [int][math]::Max(2048, $contextSize / 2)
        $memory = Get-ModelEstimatedMemory -Model $Model -ContextSize $contextSize
    }

    $cpuOnly = [bool](Resolve-ProfileValue -Profile $Profile -Name 'cpuOnly' -Fallback $false)
    $hasGpu = [bool]$Hardware.IsDedicatedGpu -and ([double]$Hardware.DedicatedVramGB -gt 0)
    $gpuLayers = if ($cpuOnly -or -not $hasGpu) { '0' } else { 'auto' }
    $gpuReason = if ($cpuOnly) {
        'CPU Only profile selected.'
    } elseif (-not $hasGpu) {
        'No dedicated GPU with usable reported VRAM was detected; llama.cpp will use CPU inference.'
    } else {
        'Auto GPU offload: llama.cpp is allowed to fit the model to the selected dedicated GPU instead of using a hand-tuned layer count.'
    }

    $logical = [math]::Max(1, [int]$Hardware.LogicalCores)
    if ($logical -le 4) {
        $threads = [math]::Max(1, $logical - 1)
    } else {
        $threads = [math]::Max(1, [math]::Min($logical - 2, [math]::Floor($logical * 0.75)))
    }

    $batchSize = switch ($hardwareTier) {
        'entry' { 128 }
        'light-laptop' { 192 }
        'laptop' { 256 }
        'standard' { 512 }
        'high' { 768 }
        default { 1024 }
    }
    if ($contextSize -ge 32768) { $batchSize = [math]::Min($batchSize, 512) }
    elseif ($contextSize -ge 16384) { $batchSize = [math]::Min($batchSize, 512) }

    $ubatchSize = switch ($hardwareTier) {
        'entry' { 64 }
        'light-laptop' { 96 }
        'laptop' { 128 }
        'standard' { 256 }
        'high' { 384 }
        default { 512 }
    }
    if ($contextSize -ge 32768) { $ubatchSize = [math]::Min($ubatchSize, 256) }

    $profileParallel = [int](Resolve-ProfileValue -Profile $Profile -Name 'parallel' -Fallback 1)
    $parallel = [math]::Min($profileParallel, if ($hardwareTier -in @('high', 'enthusiast')) { 2 } else { 1 })

    $flashMode = [string](Resolve-ProfileValue -Profile $Profile -Name 'flashAttention' -Fallback 'auto')
    if ($cpuOnly) { $flashMode = 'off' }

    $numa = [bool](Resolve-ProfileValue -Profile $Profile -Name 'numa' -Fallback ($Hardware.ProcessorCount -gt 1 -and $Hardware.PhysicalCores -ge 24))
    $mlock = [bool](Resolve-ProfileValue -Profile $Profile -Name 'mlock' -Fallback ($hardwareTier -in @('high', 'enthusiast') -and $availableRam -ge 8))
    $noMmap = [bool](Resolve-ProfileValue -Profile $Profile -Name 'noMmap' -Fallback $false)

    $kvTypeK = [string](Resolve-ProfileValue -Profile $Profile -Name 'cacheTypeK' -Fallback 'q8_0')
    $kvTypeV = [string](Resolve-ProfileValue -Profile $Profile -Name 'cacheTypeV' -Fallback 'q8_0')
    $fit = [bool](Resolve-ProfileValue -Profile $Profile -Name 'fit' -Fallback (-not $cpuOnly))
    if ($cpuOnly) { $fit = $false }
    $fitTarget = if ($fit) { [int](Resolve-ProfileValue -Profile $Profile -Name 'fitTargetMiB' -Fallback (Get-AdaptiveFitTargetMiB -Hardware $Hardware)) } else { 0 }
    $cacheRam = [int](Resolve-ProfileValue -Profile $Profile -Name 'cacheRamMiB' -Fallback (Get-AdaptiveCacheRamMiB -Hardware $Hardware))

    $reasoning = [string](Resolve-ProfileValue -Profile $Profile -Name 'reasoning' -Fallback 'auto')
    $reasoningBudget = if ($Model.reasoning) {
        [int](Resolve-ProfileValue -Profile $Profile -Name 'reasoningBudget' -Fallback (Get-AdaptiveReasoningBudget -Hardware $Hardware -ProfileId $Profile.id))
    } else { 0 }

    $temperature = [double](Resolve-ProfileValue -Profile $Profile -Name 'temperature' -Fallback $Model.defaultTemperature)
    $topP = [double](Resolve-ProfileValue -Profile $Profile -Name 'topP' -Fallback $Model.defaultTopP)
    $topK = [int](Resolve-ProfileValue -Profile $Profile -Name 'topK' -Fallback $Model.defaultTopK)
    $minP = [double](Resolve-ProfileValue -Profile $Profile -Name 'minP' -Fallback $Model.defaultMinP)
    $repeatPenalty = [double](Resolve-ProfileValue -Profile $Profile -Name 'repeatPenalty' -Fallback $Model.defaultRepeatPenalty)
    $presencePenalty = [double](Resolve-ProfileValue -Profile $Profile -Name 'presencePenalty' -Fallback $Model.defaultPresencePenalty)

    $contextReason = if ($contextSize -lt $profileTargetContext) {
        if ($contextSize -lt $hardwareContextCeiling) {
            "Reduced from the profile target because the model's context limit is $modelLimit."
        } else {
            "Reduced from the profile target to match the detected hardware tier ($hardwareTier). Context is still available for this model, but larger values would increase RAM pressure." 
        }
    } else {
        'Fits the model limit and the detected hardware tier.'
    }

    if ($availableRam -gt 0 -and $availableRam -lt 3) {
        $contextReason += ' Current free RAM is low, so close unused applications before using larger contexts.'
    }

    $recommendation = [ordered]@{
        HardwareTier = $hardwareTier
        ContextSize = [int](Resolve-ProfileValue -Profile $Profile -Name 'contextSize' -Fallback $contextSize)
        GpuLayers = [string](Resolve-ProfileValue -Profile $Profile -Name 'gpuLayers' -Fallback $gpuLayers)
        Threads = [int](Resolve-ProfileValue -Profile $Profile -Name 'threads' -Fallback $threads)
        BatchSize = [int](Resolve-ProfileValue -Profile $Profile -Name 'batchSize' -Fallback $batchSize)
        UbatchSize = [int](Resolve-ProfileValue -Profile $Profile -Name 'ubatchSize' -Fallback $ubatchSize)
        Parallel = [int]$parallel
        FlashAttention = $flashMode
        Numa = $numa
        Mlock = $mlock
        NoMmap = $noMmap
        CacheTypeK = $kvTypeK
        CacheTypeV = $kvTypeV
        CacheRamMiB = $cacheRam
        CacheReuse = 256
        Fit = $fit
        FitTargetMiB = $fitTarget
        Jinja = [bool](Resolve-ProfileValue -Profile $Profile -Name 'jinja' -Fallback $false)
        ReasoningFormat = [string](Resolve-ProfileValue -Profile $Profile -Name 'reasoningFormat' -Fallback '')
        Reasoning = $reasoning
        ReasoningBudget = $reasoningBudget
        Temperature = $temperature
        TopP = $topP
        TopK = $topK
        MinP = $minP
        RepeatPenalty = $repeatPenalty
        PresencePenalty = $presencePenalty
        Estimated = Get-ModelEstimatedMemory -Model $Model -ContextSize ([int](Resolve-ProfileValue -Profile $Profile -Name 'contextSize' -Fallback $contextSize))
        Reasons = [ordered]@{
            HardwareTier = "Auto-selected from installed RAM ($($Hardware.InstalledRamGB) GB), dedicated VRAM ($($Hardware.DedicatedVramGB) GB), and CPU threads ($($Hardware.LogicalCores))."
            ContextSize = $contextReason
            GpuLayers = $gpuReason
            Threads = 'Uses most CPU capacity while reserving some headroom for Windows, the IDE/browser, and HTTP server overhead.'
            BatchSize = "Adaptive logical/physical batch sizes for hardware tier $hardwareTier; larger contexts cap the batch to limit memory spikes."
            Parallel = if ($parallel -gt 1) { 'Hardware has enough memory to permit limited concurrent requests.' } else { 'Single-request operation is preferred for interactive coding and lower-memory systems.' }
            FlashAttention = if ($flashMode -eq 'auto') { 'Delegated to llama.cpp so the active backend decides whether flash attention is supported.' } elseif ($flashMode -eq 'on') { 'Profile explicitly enables flash attention.' } else { 'Disabled by profile or CPU-only mode.' }
            Cache = "Prompt-cache RAM ceiling is $cacheRam MiB; this is a maximum, not an upfront allocation. KV cache uses $kvTypeK/$kvTypeV."
            FitTarget = if ($fitTarget -gt 0) { "Keeps approximately $fitTarget MiB per selected device as llama.cpp's fit safety margin." } else { 'GPU fitting is not used because the profile is CPU-only.' }
            Reasoning = if ($reasoning -eq 'off') { 'Thinking/reasoning is disabled for faster everyday coding.' } elseif ($reasoning -eq 'on') { "Thinking is enabled with an adaptive budget of $reasoningBudget tokens to avoid unrestricted overthinking on smaller hardware." } else { 'Reasoning is left to the model/chat-template default.' }
        }
    }

    [pscustomobject]$recommendation
}

function Get-RecommendedModelId {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Hardware,
        [Parameter(Mandatory)][array]$Models
    )

    if (-not $Models -or $Models.Count -eq 0) { return $null }

    $ranked = $Models | ForEach-Object {
        $score = 0
        if ($_.defaultProfile -eq 'chat') { $score += 40 }
        if ($_.roles -contains 'general') { $score += 30 }
        if ($_.roles -contains 'fast') { $score += 15 }
        if ($_.capabilities -and $_.capabilities.coding) { $score += 2 }
        if ([double]$_.parametersB -le 2.0) { $score += 10 }
        [pscustomobject]@{ Model = $_; Score = $score }
    } | Sort-Object -Property @{Expression='Score';Descending=$true}, @{Expression='Model.parametersB';Descending=$false}

    return $ranked[0].Model.id
}

Export-ModuleMember -Function Get-LaunchRecommendation, Get-ModelEstimatedMemory, Get-RecommendedModelId
