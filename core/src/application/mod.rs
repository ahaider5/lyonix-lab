use crate::domain::*;

pub mod configuration;
pub mod service;

#[derive(Debug, thiserror::Error)]
pub enum RecommendationError {
    #[error("no feasible model artifact was found")]
    NoFeasibleArtifact,
}

/// Advisory estimate only. Uses model-provided KV architecture hints when available
/// instead of approximating KV memory from parameter count alone.
pub fn memory_estimate_gib(model: &ModelDefinition, artifact_size_bytes: Option<u64>, context: u32) -> f64 {
    let file_gib = artifact_size_bytes
        .map(|n| n as f64 / 1024_f64.powi(3))
        .unwrap_or_else(|| model.parameters_b * 0.52);

    let kv_f16_bpt = model
        .runtime_hints
        .kv_cache_bytes_per_token_f16
        .or_else(|| {
            match (
                model.runtime_hints.full_attention_layers,
                model.runtime_hints.kv_heads,
                model.runtime_hints.head_dim,
            ) {
                (Some(layers), Some(kv_heads), Some(head_dim)) => Some(
                    layers as u64 * kv_heads as u64 * head_dim as u64 * 2 * 2,
                ),
                _ => None,
            }
        })
        .unwrap_or((model.parameters_b * 16384.0) as u64);

    // The current default policy uses q8_0 KV. Treat it as roughly half the
    // f16 footprint; exact allocator/block overhead remains runtime-specific.
    let kv = (kv_f16_bpt as f64 * 0.5 * context as f64) / 1024_f64.powi(3);
    let vision = if model.capabilities.vision { 0.75 } else { 0.0 };
    let runtime_overhead = (0.50_f64).max(file_gib * 0.12);
    file_gib + kv + vision + runtime_overhead
}

pub fn feasible(
    hardware: &HardwareSnapshot,
    model: &ModelDefinition,
    artifact: Option<&ModelArtifact>,
    installed: Option<&InstalledModel>,
    runtime_id: &str,
    context: u32,
    requirements: CapabilityRequirements,
) -> FeasibilityResult {
    let mut reasons = Vec::new();
    if artifact.is_none() {
        reasons.push(RejectionReason { code: RejectionCode::MissingArtifact, message: "No installed artifact is available.".into() });
    }
    if requirements.vision && !model.capabilities.vision {
        reasons.push(RejectionReason { code: RejectionCode::UnsupportedCapability, message: "Selected task requires vision capability.".into() });
    }
    if requirements.reasoning && !model.capabilities.reasoning {
        reasons.push(RejectionReason { code: RejectionCode::UnsupportedCapability, message: "Selected task requires reasoning capability.".into() });
    }
    if requirements.tools && !model.capabilities.tool_calling {
        reasons.push(RejectionReason { code: RejectionCode::UnsupportedCapability, message: "Selected task requires tool-calling capability.".into() });
    }
    if model.context_limit < context {
        reasons.push(RejectionReason { code: RejectionCode::ContextExceedsModelLimit, message: format!("Requested context {context} exceeds model limit {}.", model.context_limit) });
    }
    if runtime_id != "llama.cpp" {
        reasons.push(RejectionReason { code: RejectionCode::RuntimeIncompatible, message: format!("Runtime {runtime_id} is not implemented in the initial core." )});
    }
    if let Some(installed) = installed {
        // Prefer the authoritative artifact metadata; measure the installed file
        // only when the artifact does not declare a size.
        let artifact_size = artifact
            .and_then(|a| a.size_bytes)
            .or_else(|| std::fs::metadata(&installed.local_path).ok().map(|m| m.len()));
        let estimate = memory_estimate_gib(model, artifact_size, context);
        // Safety headroom keeps a margin of RAM free beyond the estimate.
        if hardware.available_ram_gib() > 0.0 && estimate > hardware.available_ram_gib() - 1.5 {
            reasons.push(RejectionReason { code: RejectionCode::InsufficientRam, message: format!("Estimated memory {estimate:.1} GiB exceeds available RAM with safety headroom." )});
        }
    }
    FeasibilityResult { feasible: reasons.is_empty(), reasons }
}

pub fn build_recommendation(
    hardware: &HardwareSnapshot,
    models: &[(&ModelDefinition, &ModelArtifact, Option<&InstalledModel>)],
    profile: &TaskProfile,
    runtime_id: &str,
) -> RecommendationResult {
    let base_context = profile.target_context.min(hardware_context_ceiling(hardware));
    let mut feasible_candidates: Vec<CandidateScore> = Vec::new();
    let mut rejected = Vec::new();

    for (model, artifact, installed) in models {
        let model_recommended_context = model.recommended_context.unwrap_or(base_context);
        let candidate_context = base_context.min(model_recommended_context).min(model.context_limit);
        let f = feasible(
            hardware,
            model,
            Some(*artifact),
            *installed,
            runtime_id,
            candidate_context,
            CapabilityRequirements {
                vision: matches!(profile.task, TaskKind::Vision),
                reasoning: matches!(profile.task, TaskKind::Reasoning | TaskKind::Agent) && profile.allow_reasoning,
                tools: matches!(profile.task, TaskKind::Agent),
            },
        );
        if !f.feasible {
            rejected.extend(f.reasons);
            continue;
        }

        let mut score = 0.0;
        let mut reasons = Vec::new();
        let has_role = |name: &str| model.roles.iter().any(|r| r.eq_ignore_ascii_case(name));

        match profile.task {
            TaskKind::Chat => {
                if has_role("general") { score += 30.0; reasons.push("general-purpose role".into()); }
                if has_role("fast") { score += 15.0; reasons.push("fast-inference role".into()); }
            }
            TaskKind::Coding => {
                if model.capabilities.coding { score += 30.0; reasons.push("coding capability".into()); }
                if has_role("coding") { score += 15.0; reasons.push("coding specialist role".into()); }
            }
            TaskKind::Reasoning => {
                if model.capabilities.reasoning { score += 35.0; reasons.push("reasoning capability".into()); }
                if has_role("reasoning") { score += 15.0; reasons.push("reasoning role".into()); }
            }
            TaskKind::Vision => {
                if model.capabilities.vision { score += 40.0; reasons.push("vision capability".into()); }
            }
            TaskKind::LongContext => {
                if model.context_limit >= 32768 { score += 25.0; reasons.push("large context capability".into()); }
                if model.context_limit >= 131072 { score += 15.0; reasons.push("very large context capability".into()); }
            }
            TaskKind::Agent => {
                if model.capabilities.tool_calling { score += 35.0; reasons.push("tool-calling capability".into()); }
                if has_role("agent") { score += 20.0; reasons.push("agent role".into()); }
            }
        }

        if profile.low_latency && has_role("fast") { score += 10.0; reasons.push("low-latency preference".into()); }
        if model.parameters_b <= 2.0 { score += 12.0; reasons.push("small model footprint".into()); }
        else if model.parameters_b <= 4.0 { score += 6.0; reasons.push("moderate model footprint".into()); }
        if profile.preferred_reasoning != ReasoningMode::Off && model.capabilities.reasoning { score += 5.0; }
        if model.default_profile.as_deref() == Some(profile.id.as_str()) { score += 8.0; reasons.push("matches model default profile".into()); }
        if installed.is_some() { score += 10.0; reasons.push("artifact already installed".into()); }
        if hardware.dedicated_gpu_gib() > 0.0 && model.parameters_b <= 8.0 { score += 5.0; }

        feasible_candidates.push(CandidateScore { artifact_id: artifact.id.clone(), score, reasons });
    }

    feasible_candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let winner = feasible_candidates.first().and_then(|candidate| models.iter().find(|(_, artifact, _)| artifact.id == candidate.artifact_id));
    let (recommended_model, recommended_artifact, installed) = match winner {
        Some((m, a, i)) => (Some((*m).clone()), Some((*a).clone()), *i),
        None => (None, None, None),
    };

    let configuration = recommended_model.as_ref().map(|m| {
        let context = base_context.min(m.recommended_context.unwrap_or(base_context)).min(m.context_limit);
        let reasoning_mode = match profile.preferred_reasoning {
            ReasoningMode::Off => "off",
            ReasoningMode::On => "on",
            ReasoningMode::Auto => "auto",
        };
        RuntimeConfiguration {
            context,
            batch: None,
            micro_batch: None,
            parallelism: profile.preferred_parallelism.max(1),
            device_policy: Some("runtime_auto".into()),
            gpu_offload: if profile.cpu_only { GpuOffloadPolicy::CpuOnly } else { GpuOffloadPolicy::Auto },
            memory: MemoryPolicy { fit: !profile.cpu_only, fit_target_mib: Some(fit_target_mib(hardware)), cache_ram_mib: Some(cache_ram_mib(hardware)) },
            kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() },
            reasoning: ReasoningPolicy {
                mode: if m.capabilities.reasoning { reasoning_mode.into() } else { "off".into() },
                budget: if m.capabilities.reasoning && profile.preferred_reasoning != ReasoningMode::Off { Some(reasoning_budget(hardware)) } else { None },
            },
            sampling: SamplingPolicy { temperature: m.default_temperature, top_p: m.default_top_p, top_k: m.default_top_k, min_p: m.default_min_p, repeat_penalty: m.default_repeat_penalty, presence_penalty: m.default_presence_penalty },
            network: NetworkPolicy::default(),
            advanced: AdvancedSettings::default(),
        }
    });

    let confidence = match feasible_candidates.first() {
        Some(best) if feasible_candidates.len() > 1 => (best.score - feasible_candidates[1].score).clamp(0.0, 100.0) / 100.0,
        Some(_) => 0.8,
        None => 0.0,
    };

    let reasons = match (&recommended_model, installed) {
        (Some(model), Some(installed)) => vec![format!("Selected {} from an installed artifact at {}.", model.display_name, installed.local_path.display())],
        (Some(model), None) => vec![format!("Selected {} as the highest-scoring feasible candidate.", model.display_name)],
        _ => vec![],
    };
    let recommendation_runtime_id = recommended_model.as_ref().map(|_| runtime_id.into());

    RecommendationResult {
        recommended_model,
        recommended_artifact,
        runtime_id: recommendation_runtime_id,
        configuration,
        confidence,
        reasons,
        warnings: vec!["Performance remains heuristic until empirical benchmark data is available.".into()],
        alternatives: feasible_candidates.into_iter().skip(1).collect(),
        rejected,
    }
}

fn hardware_context_ceiling(hardware: &HardwareSnapshot) -> u32 {
    let ram = hardware.total_ram_gib();
    if ram < 8.0 { 4096 } else if ram < 16.0 { 8192 } else if ram < 24.0 { 16384 } else if ram < 64.0 { 32768 } else { 65536 }
}
fn fit_target_mib(hardware: &HardwareSnapshot) -> u64 {
    let vram = hardware.dedicated_gpu_gib();
    if vram <= 0.0 { 0 } else if vram < 3.0 { 256 } else if vram < 6.0 { 512 } else if vram < 12.0 { 768 } else { 1024 }
}
fn cache_ram_mib(hardware: &HardwareSnapshot) -> u64 {
    let ram = hardware.total_ram_gib();
    if ram < 8.0 { 512 } else if ram < 16.0 { 768 } else if ram < 24.0 { 1536 } else if ram < 32.0 { 2048 } else if ram < 64.0 { 4096 } else { 8192 }
}
fn reasoning_budget(hardware: &HardwareSnapshot) -> u32 {
    let vram = hardware.dedicated_gpu_gib();
    let ram = hardware.total_ram_gib();
    if vram < 3.0 || ram < 12.0 { 512 } else if vram < 6.0 || ram < 24.0 { 1024 } else if vram < 12.0 || ram < 32.0 { 2048 } else { 4096 }
}
