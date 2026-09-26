export type RuntimeState = 'stopped' | 'starting' | 'loading' | 'ready' | 'stopping' | 'failed' | 'unknown';

export interface HardwareSnapshot {
  os: string;
  cpu: { vendor: string | null; model: string; architecture: string; physical_cores: number; logical_cores: number };
  memory: { total_bytes: number; available_bytes: number; used_bytes: number; pressure_percent: number | null };
  gpus: Array<{
    vendor: string;
    model: string;
    hardware_id: string | null;
    dedicated_memory_bytes: number | null;
    shared_memory_bytes: number | null;
    memory_kind: string;
    backend: string | null;
    driver: string | null;
    runtime_devices: Array<RuntimeDevice>;
  }>;
  runtime_devices: RuntimeDevice[];
  collected_at_epoch_seconds: number;
}

export interface RuntimeDevice {
  runtime_id: string;
  device_id: string;
  backend: string;
  name: string;
  total_memory_bytes: number | null;
  available_memory_bytes: number | null;
}

export interface RuntimeInfo {
  id: string;
  display_name: string;
  executable: string | null;
  version: string | null;
  capabilities: Record<string, boolean>;
}

export interface InstalledModel {
  artifact_id: string;
  model_definition_id: string;
  local_path: string;
  verified: boolean;
}

export interface RuntimeStatus {
  runtime_id: string;
  state: RuntimeState;
  pid: number | null;
  endpoint: string | null;
  model_id: string | null;
  artifact_id: string | null;
  effective_configuration?: unknown;
  devices: RuntimeDevice[];
  message: string | null;
}


export interface UserConfiguration {
  version: number;
  model_roots: string[];
  llama_server_path: string | null;
  selected_model_id: string | null;
  selected_profile_id: string | null;
  network: { host: string; port: number } | null;
}
