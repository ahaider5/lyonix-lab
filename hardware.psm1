function ConvertTo-GB {
    param([Nullable[double]]$Bytes)

    if ($null -eq $Bytes -or $Bytes -le 0) {
        return 0
    }

    return [math]::Round($Bytes / 1GB, 2)
}

function Get-GpuVendor {
    param([string]$Name)

    if ($Name -match 'NVIDIA|GeForce|RTX|GTX|Quadro|Tesla') { return 'NVIDIA' }
    if ($Name -match 'AMD|Radeon|RX |FirePro') { return 'AMD' }
    if ($Name -match 'Intel|Arc|Iris|UHD') { return 'Intel' }
    if ([string]::IsNullOrWhiteSpace($Name)) { return 'Unknown' }
    return 'Other'
}

function Get-IsDedicatedGpu {
    param([pscustomobject]$Gpu)

    if ($Gpu.Vendor -eq 'Intel') {
        return ($Gpu.Model -match '\bArc\b')
    }

    if ($Gpu.Vendor -eq 'AMD') {
        # APUs often report a small adapter reservation even though graphics
        # memory is shared with system RAM. Treat >=2 GB reported VRAM as the
        # conservative threshold for dedicated Radeon-class offload.
        return ([double]$Gpu.DedicatedVramGB -ge 2)
    }

    if ($Gpu.Vendor -eq 'NVIDIA') { return ([double]$Gpu.DedicatedVramGB -gt 0) }
    return $false
}

function Get-LauncherHardware {
    [CmdletBinding()]
    param(
        [string]$LlamaServer = ''
    )

    $processors = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction SilentlyContinue)
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction SilentlyContinue
    $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction SilentlyContinue
    $gpus = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction SilentlyContinue)

    $cpuName = ($processors | Select-Object -First 1 -ExpandProperty Name)
    $physicalCores = ($processors | Measure-Object -Property NumberOfCores -Sum).Sum
    $logicalCores = ($processors | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum
    $processorCount = if ($computer.NumberOfProcessors) { [int]$computer.NumberOfProcessors } else { 1 }

    $gpuInfo = foreach ($gpu in $gpus) {
        $adapterBytes = 0
        if ($gpu.AdapterRAM -and $gpu.AdapterRAM -gt 0) {
            $adapterBytes = [uint64]$gpu.AdapterRAM
        }

        $info = [pscustomobject]@{
            Vendor = Get-GpuVendor -Name $gpu.Name
            Model = if ($gpu.Name) { $gpu.Name.Trim() } else { 'Unknown GPU' }
            DedicatedVramGB = ConvertTo-GB -Bytes $adapterBytes
            DriverVersion = $gpu.DriverVersion
            PnpDeviceId = $gpu.PNPDeviceID
        }
        $info | Add-Member -NotePropertyName IsDedicated -NotePropertyValue (Get-IsDedicatedGpu -Gpu $info) -Force
        $info
    }

    $discreteGpus = @($gpuInfo | Where-Object { $_.IsDedicated } | Sort-Object -Property @{ Expression = { $_.DedicatedVramGB }; Descending = $true }, Model)
    $primaryGpu = if ($discreteGpus.Count -gt 0) {
        $discreteGpus[0]
    } else {
        $gpuInfo | Sort-Object -Property @{ Expression = { $_.DedicatedVramGB }; Descending = $true }, Model | Select-Object -First 1
    }

    $hardware = [pscustomobject]@{
        CpuModel = if ($cpuName) { $cpuName.Trim() } else { 'Unknown CPU' }
        PhysicalCores = [int]($physicalCores -as [int])
        LogicalCores = [int]($logicalCores -as [int])
        ProcessorCount = $processorCount
        InstalledRamGB = ConvertTo-GB -Bytes $computer.TotalPhysicalMemory
        AvailableRamGB = if ($os.FreePhysicalMemory) { [math]::Round(($os.FreePhysicalMemory * 1KB) / 1GB, 2) } else { 0 }
        GpuVendor = if ($primaryGpu) { $primaryGpu.Vendor } else { 'Unknown' }
        GpuModel = if ($primaryGpu) { $primaryGpu.Model } else { 'No GPU detected' }
        DedicatedVramGB = if ($primaryGpu) { $primaryGpu.DedicatedVramGB } else { 0 }
        IsDedicatedGpu = if ($primaryGpu) { [bool]$primaryGpu.IsDedicated } else { $false }
        OperatingSystem = if ($os.Caption) { "$($os.Caption) $($os.Version)" } else { [System.Environment]::OSVersion.VersionString }
        AllGpus = @($gpuInfo)
    }

    $hardware | Add-Member -NotePropertyName LlamaDevices -NotePropertyValue @() -Force
    if (-not [string]::IsNullOrWhiteSpace($LlamaServer)) {
        try {
            $command = Get-Command $LlamaServer -ErrorAction Stop
            $output = @(& $command.Source --list-devices 2>&1)
            $parsed = foreach ($line in $output) {
                $text = [string]$line
                if ($text -match '^\s*(?<id>[^:]+):\s*(?<name>.*?)\s+\((?<total>\d+)\s+MiB,\s+(?<free>\d+)\s+MiB free\)') {
                    [pscustomobject]@{
                        Id = $matches.id.Trim()
                        Name = $matches.name.Trim()
                        TotalMiB = [int]$matches.total
                        FreeMiB = [int]$matches.free
                        IsGpu = $true
                    }
                }
            }
            $hardware.LlamaDevices = @($parsed)
        } catch {
            # Device enumeration is advisory. The launcher can still operate
            # using the detected Windows GPU information and llama.cpp defaults.
            $hardware.LlamaDevices = @()
        }
    }

    return $hardware
}

function Select-PreferredLlamaDevice {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Hardware
    )

    if (-not $Hardware.LlamaDevices -or $Hardware.LlamaDevices.Count -eq 0) {
        return $null
    }

    $candidates = @($Hardware.LlamaDevices | Where-Object { $_.IsGpu })
    if ($candidates.Count -eq 0) { return $null }

    # Prefer a llama.cpp device whose name matches the best dedicated Windows GPU.
    if ($Hardware.GpuModel) {
        $matched = $candidates | Where-Object { $_.Name -match [regex]::Escape($Hardware.GpuModel) } | Select-Object -First 1
        if ($matched) { return $matched }
    }

    switch ($Hardware.GpuVendor) {
        'NVIDIA' {
            $matched = $candidates | Where-Object { $_.Name -match 'NVIDIA|GeForce|RTX|GTX|Quadro|Tesla' } | Select-Object -First 1
            if ($matched) { return $matched }
        }
        'AMD' {
            $matched = $candidates | Where-Object { $_.Name -match 'AMD|Radeon|RX |FirePro' } | Select-Object -First 1
            if ($matched) { return $matched }
        }
        'Intel' {
            $matched = $candidates | Where-Object { $_.Name -match 'Intel|UHD|Iris|Arc' } | Select-Object -First 1
            if ($matched) { return $matched }
        }
    }

    # As a last resort, choose the llama.cpp GPU with the most reported VRAM.
    return $candidates | Sort-Object FreeMiB -Descending | Select-Object -First 1
}

Export-ModuleMember -Function Get-LauncherHardware, Select-PreferredLlamaDevice
