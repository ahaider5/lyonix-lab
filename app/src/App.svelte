<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './lib/api';
  import type { HardwareSnapshot, InstalledModel, RuntimeInfo, RuntimeStatus, UserConfiguration } from './lib/types';

  type View = 'Dashboard' | 'Models' | 'Runtime' | 'Profiles' | 'Settings' | 'Diagnostics';
  const views: View[] = ['Dashboard', 'Models', 'Runtime', 'Profiles', 'Settings', 'Diagnostics'];
  let view: View = 'Dashboard';
  let hardware: HardwareSnapshot | null = null;
  let runtimes: RuntimeInfo[] = [];
  let models: InstalledModel[] = [];
  let runtime: RuntimeStatus = { runtime_id: 'llama.cpp', state: 'stopped', pid: null, endpoint: 'http://127.0.0.1:8081/v1', model_id: null, artifact_id: null, devices: [], message: 'No managed runtime is running.' };
  let loading = true;
  let busy = false;
  let error = '';
  let config: UserConfiguration = { version: 1, model_roots: [], llama_server_path: null, selected_model_id: null, selected_profile_id: null, network: null };
  let configRoots = '';
  let configSaved = false;

  async function refresh() {
    loading = true;
    error = '';
    try {
      const snapshot = await api.refresh();
      hardware = snapshot.hardware;
      runtimes = snapshot.runtimes;
      models = snapshot.models;
      runtime = snapshot.runtime;
      config = await api.getConfiguration();
      configRoots = config.model_roots.join('\n');
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  async function start() {
    busy = true;
    error = '';
    try { runtime = await api.startRuntime(); }
    catch (err) { error = String(err); }
    finally { busy = false; }
  }

  async function saveSettings() {
    configSaved = false;
    const roots = configRoots.split(/\r?\n/).map(v => v.trim()).filter(Boolean);
    config = await api.saveConfiguration({ ...config, model_roots: roots });
    configSaved = true;
    await refresh();
  }

  async function stop() {
    busy = true;
    error = '';
    try { runtime = await api.stopRuntime(); }
    catch (err) { error = String(err); }
    finally { busy = false; }
  }

  onMount(() => {
    refresh();
    const timer = window.setInterval(async () => {
      if (runtime.state !== 'stopped' && runtime.state !== 'failed') {
        try { runtime = await api.getRuntimeStatus(); } catch { /* foreground refresh surfaces actionable errors */ }
      }
    }, 2000);
    return () => window.clearInterval(timer);
  });

  $: ramGiB = hardware ? (hardware.memory.total_bytes / 1073741824).toFixed(1) : '—';
  $: availableRamGiB = hardware ? (hardware.memory.available_bytes / 1073741824).toFixed(1) : '—';
  $: primaryGpu = hardware?.gpus[0];
</script>

<div class="shell">
  <header class="topbar">
    <div class="brand"><span class="mark">L</span><div><strong>LYONIX-LAB</strong><small>local inference manager</small></div></div>
    <div class="top-actions">
      <span class:ok={runtime.state === 'ready'} class="status-dot"></span>
      <span>{runtime.state}</span>
      <button onclick={refresh} disabled={loading}>Refresh</button>
    </div>
  </header>

  <div class="body">
    <aside class="sidebar">
      <nav>
        {#each views as item}
          <button class:active={view === item} onclick={() => view = item}>{item}</button>
        {/each}
      </nav>
      <div class="side-note"><span>SAFE DEFAULTS</span><p>Localhost-only API<br />Runtime tools disabled</p></div>
    </aside>

    <main>
      {#if error}<div class="error">{error}</div>{/if}

      {#if view === 'Dashboard'}
        <section class="hero">
          <div><span class="eyebrow">CONTROL PLANE</span><h1>Local inference, without the bloat.</h1><p>Hardware-aware runtime management with explicit, inspectable decisions.</p></div>
          <div class="hero-action">
            {#if runtime.state === 'ready' || runtime.state === 'loading' || runtime.state === 'starting'}
              <button class="danger" onclick={stop} disabled={busy}>Stop runtime</button>
            {:else}
              <button class="primary" onclick={start} disabled={busy}>Start runtime</button>
            {/if}
          </div>
        </section>

        <section class="grid four">
          <article class="card metric"><span>CPU</span><strong>{hardware?.cpu.model ?? 'Detecting…'}</strong><small>{hardware?.cpu.logical_cores ?? '—'} logical threads</small></article>
          <article class="card metric"><span>MEMORY</span><strong>{ramGiB} GiB</strong><small>{availableRamGiB} GiB available</small></article>
          <article class="card metric"><span>GPU</span><strong>{primaryGpu?.model ?? 'None detected'}</strong><small>{primaryGpu ? `${((primaryGpu.dedicated_memory_bytes ?? 0) / 1073741824).toFixed(1)} GiB runtime-reported` : 'CPU fallback'}</small></article>
          <article class="card metric"><span>MODEL INVENTORY</span><strong>{models.length}</strong><small>installed artifacts</small></article>
        </section>

        <section class="grid two">
          <article class="card large"><div class="card-head"><div><span class="eyebrow">RUNTIME</span><h2>{runtime.runtime_id}</h2></div><span class="pill" class:good={runtime.state === 'ready'}>{runtime.state}</span></div><div class="details"><div><span>Endpoint</span><code>{runtime.endpoint ?? 'Not running'}</code></div><div><span>Model</span><strong>{runtime.model_id ?? '—'}</strong></div><div><span>PID</span><strong>{runtime.pid ?? '—'}</strong></div><div><span>Message</span><strong>{runtime.message ?? 'No active runtime'}</strong></div></div></article>
          <article class="card large"><span class="eyebrow">RUNTIMES</span><h2>Detected backends</h2>{#if runtimes.length === 0}<p class="muted">No supported runtime detected.</p>{:else}{#each runtimes as r}<div class="runtime-row"><div><strong>{r.display_name}</strong><small>{r.version ?? 'version unavailable'}</small></div><span class="pill">detected</span></div>{/each}{/if}</article>
        </section>
      {:else if view === 'Models'}
        <section class="page-head"><span class="eyebrow">MODEL INVENTORY</span><h1>Installed models</h1><p>The catalog and resolver remain backend-owned. This view only renders resolved inventory.</p></section>
        <div class="card table-card"><div class="table-head"><span>Artifact</span><span>Definition</span><span>Verified</span><span>Path</span></div>{#each models as model}<div class="table-row"><strong>{model.artifact_id}</strong><span>{model.model_definition_id}</span><span>{model.verified ? 'Verified' : 'Unverified'}</span><code>{model.local_path}</code></div>{/each}{#if models.length === 0}<div class="empty">No installed model artifacts found.</div>{/if}</div>
      {:else if view === 'Runtime'}
        <section class="page-head"><span class="eyebrow">RUNTIME CONTROL</span><h1>llama.cpp</h1><p>Managed process lifecycle. Readiness follows runtime health, not process existence.</p></section>
        <div class="grid two"><article class="card large"><span class="eyebrow">STATUS</span><div class="big-state">{runtime.state}</div><p>{runtime.message ?? 'No message.'}</p><div class="button-row"><button class="primary" onclick={start} disabled={busy}>Start</button><button class="danger" onclick={stop} disabled={busy}>Stop</button></div></article><article class="card large"><span class="eyebrow">NETWORK</span><h2>127.0.0.1:8081</h2><p>LAN exposure is not enabled in this milestone.</p><div class="security-line">Built-in runtime tools: <strong>OFF</strong></div></article></div>
      {:else if view === 'Profiles'}
        <section class="page-head"><span class="eyebrow">TASK INTENT</span><h1>Profiles</h1><p>Profiles describe intent; hardware policy translates intent into runtime configuration.</p></section>
        <div class="profile-grid">{#each ['Chat / Fast','Coding','Reasoning','Agent / Hermes','Low Memory','CPU Only'] as profile}<article class="card profile"><h2>{profile}</h2><p>{profile === 'Coding' ? 'Prefer code-capable models, low latency, stable output and moderate context.' : 'Semantic task intent; runtime values remain policy-derived.'}</p></article>{/each}</div>
      {:else if view === 'Settings'}
        <section class="page-head"><span class="eyebrow">CONFIGURATION</span><h1>Settings</h1><p>Managed configuration is authoritative. Advanced runtime-specific overrides are deliberately constrained.</p></section>
        <div class="card settings"><div><span>Default host</span><strong>127.0.0.1</strong></div><div><span>Default port</span><strong>8081</strong></div><div><span>Raw runtime arguments</span><strong>Experimental / constrained</strong></div><div><span>Runtime</span><strong>External llama.cpp</strong></div><div class="setting-wide"><span>Model roots</span><textarea bind:value={configRoots} rows="4" placeholder="One directory per line"></textarea><div class="button-row"><button class="primary" onclick={saveSettings}>Save model roots</button>{#if configSaved}<span class="saved">Saved</span>{/if}</div></div></div>
      {:else}
        <section class="page-head"><span class="eyebrow">DIAGNOSTICS</span><h1>Diagnostics</h1><p>Use this surface for actionable state. Secret values and arbitrary personal paths are intentionally not rendered here.</p></section>
        <div class="card diagnostic"><div class="button-row"><button class="primary" onclick={async () => { try { const path = await api.exportDiagnostics(); error = `Diagnostics exported to ${path}`; } catch (err) { error = String(err); } }}>Export diagnostics</button></div><pre>{JSON.stringify({ runtime, hardware, runtimes: runtimes.map(r => ({ id: r.id, version: r.version })), installedModels: models.length }, null, 2)}</pre></div>
      {/if}
    </main>
  </div>
</div>
