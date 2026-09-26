$ErrorActionPreference = 'Continue'
$BaseDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
Import-Module (Join-Path $BaseDir 'hardware.psm1') -Force

Write-Host 'LYONIX-LAB Hardware Diagnostics'
Write-Host '================================'

$llamaServer = 'llama-server'
try {
    $resolved = (Get-Command $llamaServer -ErrorAction Stop).Source
    $llamaServer = $resolved
    Write-Host "llama-server : $resolved"
} catch {
    Write-Warning 'llama-server was not found on PATH.'
}

$hardware = Get-LauncherHardware -LlamaServer $llamaServer
$preferred = Select-PreferredLlamaDevice -Hardware $hardware

Write-Host "CPU           : $($hardware.CpuModel)"
Write-Host "CPU threads   : $($hardware.LogicalCores) logical / $($hardware.PhysicalCores) physical"
Write-Host "RAM           : $($hardware.InstalledRamGB) GB installed / $($hardware.AvailableRamGB) GB available"
Write-Host "Best GPU      : $($hardware.GpuModel)"
Write-Host "GPU vendor    : $($hardware.GpuVendor)"
Write-Host "GPU VRAM      : $($hardware.DedicatedVramGB) GB reported"
Write-Host "Llama device  : $(if ($preferred) { "$($preferred.Id) ($($preferred.Name))" } else { 'not resolved' })"
Write-Host ''
Write-Host 'llama.cpp devices:'
if ($hardware.LlamaDevices.Count -gt 0) {
    foreach ($device in $hardware.LlamaDevices) {
        Write-Host ("  {0}: {1} ({2} MiB total, {3} MiB free)" -f $device.Id, $device.Name, $device.TotalMiB, $device.FreeMiB)
    }
} else {
    Write-Host '  No llama.cpp device list was available.'
}
Write-Host ''
Write-Host 'Windows GPUs:'
foreach ($gpu in $hardware.AllGpus) {
    Write-Host ("  {0} | {1} | {2} GB reported | dedicated={3}" -f $gpu.Vendor, $gpu.Model, $gpu.DedicatedVramGB, $gpu.IsDedicated)
}
