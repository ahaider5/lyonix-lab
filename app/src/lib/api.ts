import { invoke } from '@tauri-apps/api/core';
import type { HardwareSnapshot, InstalledModel, RuntimeInfo, RuntimeStatus, UserConfiguration } from './types';

export const api = {
  async getHardware(): Promise<HardwareSnapshot> {
    return invoke('get_hardware');
  },
  async getRuntimes(): Promise<RuntimeInfo[]> {
    return invoke('get_runtimes');
  },
  async getModels(): Promise<InstalledModel[]> {
    return invoke('get_installed_models');
  },
  async getRuntimeStatus(): Promise<RuntimeStatus> {
    return invoke('get_runtime_status');
  },
  async startRuntime(): Promise<RuntimeStatus> {
    return invoke('start_runtime');
  },
  async stopRuntime(): Promise<RuntimeStatus> {
    return invoke('stop_runtime');
  },
  async getConfiguration(): Promise<UserConfiguration> {
    return invoke('get_user_configuration');
  },
  async saveConfiguration(config: UserConfiguration): Promise<UserConfiguration> {
    return invoke('save_user_configuration', { config });
  },
  async exportDiagnostics(): Promise<string> {
    return invoke('export_diagnostics');
  },
  async refresh(): Promise<{ hardware: HardwareSnapshot; runtimes: RuntimeInfo[]; models: InstalledModel[]; runtime: RuntimeStatus }> {
    return invoke('refresh_snapshot');
  }
};
