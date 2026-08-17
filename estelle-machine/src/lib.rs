//! Local machine capability measurements for Estelle.
//!
//! Hardware detection and named-model fit/plan calculations are provided by
//! Alex Jones's `llmfit-core` 1.1.9, licensed under MIT. Estelle deliberately
//! exposes no llmfit catalog ranking or serving-model selection: the server's
//! Affinity remains the sole owner of that decision. The client reports only
//! what this machine can run for a model the caller already named.

use llmfit_core::LlmModel;
use llmfit_core::ModelDatabase;
use llmfit_core::PlanRequest;
use llmfit_core::SystemSpecs;
use llmfit_core::estimate_model_plan;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MachineFacts {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_cores: usize,
    pub cpu_name: String,
    pub gpu_name: Option<String>,
    pub gpu_vram_gb: Option<f64>,
    pub gpu_available_gb: Option<f64>,
    pub gpu_count: u32,
    pub unified_memory: bool,
    pub backend: String,
}

#[derive(Clone, Debug)]
pub struct Machine {
    specs: SystemSpecs,
}

impl Machine {
    pub fn facts(&self) -> MachineFacts {
        MachineFacts {
            total_ram_gb: self.specs.total_ram_gb,
            available_ram_gb: self.specs.available_ram_gb,
            cpu_cores: self.specs.total_cpu_cores,
            cpu_name: self.specs.cpu_name.clone(),
            gpu_name: self.specs.gpu_name.clone(),
            gpu_vram_gb: self.specs.total_gpu_vram_gb.or(self.specs.gpu_vram_gb),
            gpu_available_gb: self.specs.gpu_available_gb,
            gpu_count: self.specs.gpu_count,
            unified_memory: self.specs.unified_memory,
            backend: self.specs.backend.label().to_string(),
        }
    }

    pub fn summary_line(&self) -> String {
        let facts = self.facts();
        let accelerator = match (&facts.gpu_name, facts.gpu_vram_gb, facts.gpu_available_gb) {
            (Some(name), Some(pool), Some(limit)) => {
                format!("{name} · {pool:.1} GB unified pool · {limit:.1} GB Metal limit")
            }
            (Some(name), Some(vram), None) => format!("{name} · {vram:.1} GB GPU memory"),
            (Some(name), None, _) => format!("{name} · GPU memory unknown"),
            (None, _, _) => "no GPU detected".to_string(),
        };
        format!(
            "This machine · {:.1} GB RAM ({:.1} GB available) · {} CPU cores · {}",
            facts.total_ram_gb, facts.available_ram_gb, facts.cpu_cores, accelerator
        )
    }

    #[cfg(test)]
    fn from_specs(specs: SystemSpecs) -> Self {
        Self { specs }
    }
}

pub fn machine() -> Machine {
    Machine {
        specs: SystemSpecs::detect(),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Model {
    pub name: String,
    pub provider: String,
    pub parameter_count: String,
    pub parameters_raw: Option<u64>,
    pub min_ram_gb: f64,
    pub recommended_ram_gb: f64,
    pub min_vram_gb: Option<f64>,
    pub quantization: String,
    pub context_length: u32,
    pub use_case: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitLevel {
    Perfect,
    Good,
    Marginal,
    TooTight,
}

impl FitLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Perfect => "perfect",
            Self::Good => "good",
            Self::Marginal => "marginal",
            Self::TooTight => "too_tight",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Gpu,
    MoeOffload,
    CpuOffload,
    CpuOnly,
    TensorParallel,
}

impl RunMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::MoeOffload => "moe_offload",
            Self::CpuOffload => "cpu_offload",
            Self::CpuOnly => "cpu_only",
            Self::TensorParallel => "tensor_parallel",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fit {
    pub model_name: String,
    pub fit_level: FitLevel,
    pub run_mode: RunMode,
    pub memory_required_gb: f64,
    pub memory_available_gb: f64,
    pub estimated_tokens_per_second: f64,
    pub quantization: String,
    pub context_tokens: u32,
    pub estimate_notice: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FitError {
    #[error("no unique exact bundled metadata match for model: {0}")]
    UnknownModel(String),
    #[error("invalid model metadata: {0}")]
    InvalidModel(String),
    #[error("fit estimate failed: {0}")]
    Estimate(String),
}

/// Returns resource metadata only when the caller supplies one exact bundled name.
/// This function never performs fuzzy matching, ranking, or model selection.
pub fn named_model(name: &str) -> Result<Model, FitError> {
    let database = ModelDatabase::embedded();
    let mut matches = database
        .get_all_models()
        .iter()
        .filter(|model| model.name.eq_ignore_ascii_case(name));
    let model = matches
        .next()
        .ok_or_else(|| FitError::UnknownModel(name.to_string()))?;
    if matches.next().is_some() {
        return Err(FitError::UnknownModel(name.to_string()));
    }
    Ok(Model {
        name: model.name.clone(),
        provider: model.provider.clone(),
        parameter_count: model.parameter_count.clone(),
        parameters_raw: model.parameters_raw,
        min_ram_gb: model.min_ram_gb,
        recommended_ram_gb: model.recommended_ram_gb,
        min_vram_gb: model.min_vram_gb,
        quantization: model.quantization.clone(),
        context_length: model.context_length,
        use_case: model.use_case.clone(),
    })
}

pub fn fit(model: &Model, machine: &Machine) -> Result<Fit, FitError> {
    validate_model(model)?;
    let value =
        serde_json::to_value(model).map_err(|error| FitError::InvalidModel(error.to_string()))?;
    let upstream: LlmModel =
        serde_json::from_value(value).map_err(|error| FitError::InvalidModel(error.to_string()))?;
    let request = PlanRequest {
        context: upstream.context_length.min(8_192),
        quant: Some(upstream.quantization.clone()),
        target_tps: None,
        kv_quant: None,
    };
    let mut effective_specs = machine.specs.clone();
    if effective_specs.unified_memory
        && let Some(limit) = effective_specs.gpu_available_gb
    {
        effective_specs.gpu_vram_gb = Some(limit);
        effective_specs.total_gpu_vram_gb = Some(limit);
    }
    let plan =
        estimate_model_plan(&upstream, &request, &effective_specs).map_err(FitError::Estimate)?;
    let run_mode = map_run_mode(plan.current.run_mode);
    let memory_required_gb =
        upstream.estimate_memory_gb_with_kv(&plan.quantization, plan.context, plan.kv_quant);
    let memory_available_gb = match run_mode {
        RunMode::Gpu | RunMode::TensorParallel => effective_specs
            .total_gpu_vram_gb
            .or(effective_specs.gpu_vram_gb)
            .unwrap_or(0.0),
        RunMode::MoeOffload | RunMode::CpuOffload | RunMode::CpuOnly => {
            machine.specs.available_ram_gb
        }
    };

    Ok(Fit {
        model_name: plan.model_name,
        fit_level: map_fit_level(plan.current.fit_level),
        run_mode,
        memory_required_gb,
        memory_available_gb,
        estimated_tokens_per_second: plan.current.estimated_tps,
        quantization: plan.quantization,
        context_tokens: plan.context,
        estimate_notice: plan.estimate_notice,
    })
}

fn validate_model(model: &Model) -> Result<(), FitError> {
    if model.name.trim().is_empty() {
        return Err(FitError::InvalidModel("name is empty".to_string()));
    }
    if model.context_length == 0 {
        return Err(FitError::InvalidModel(
            "context_length must be greater than zero".to_string(),
        ));
    }
    for (name, value) in [
        ("min_ram_gb", model.min_ram_gb),
        ("recommended_ram_gb", model.recommended_ram_gb),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(FitError::InvalidModel(format!(
                "{name} must be a finite positive number"
            )));
        }
    }
    if model
        .min_vram_gb
        .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err(FitError::InvalidModel(
            "min_vram_gb must be a finite positive number when present".to_string(),
        ));
    }
    Ok(())
}

fn map_fit_level(level: llmfit_core::FitLevel) -> FitLevel {
    match level {
        llmfit_core::FitLevel::Perfect => FitLevel::Perfect,
        llmfit_core::FitLevel::Good => FitLevel::Good,
        llmfit_core::FitLevel::Marginal => FitLevel::Marginal,
        llmfit_core::FitLevel::TooTight => FitLevel::TooTight,
    }
}

fn map_run_mode(mode: llmfit_core::RunMode) -> RunMode {
    match mode {
        llmfit_core::RunMode::Gpu => RunMode::Gpu,
        llmfit_core::RunMode::MoeOffload => RunMode::MoeOffload,
        llmfit_core::RunMode::CpuOffload => RunMode::CpuOffload,
        llmfit_core::RunMode::CpuOnly => RunMode::CpuOnly,
        llmfit_core::RunMode::TensorParallel => RunMode::TensorParallel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmfit_core::GpuBackend;

    fn cpu_machine(available_ram_gb: f64) -> Machine {
        Machine::from_specs(SystemSpecs {
            total_ram_gb: 32.0,
            available_ram_gb,
            total_cpu_cores: 12,
            cpu_name: "Fixture CPU".to_string(),
            has_gpu: false,
            gpu_vram_gb: None,
            total_gpu_vram_gb: None,
            gpu_available_gb: None,
            gpu_name: None,
            gpu_count: 0,
            unified_memory: false,
            backend: GpuBackend::CpuX86,
            gpus: Vec::new(),
            cluster_mode: false,
            cluster_node_count: 0,
        })
    }

    fn seven_b_model() -> Model {
        Model {
            name: "Fixture-7B".to_string(),
            provider: "fixture".to_string(),
            parameter_count: "7B".to_string(),
            parameters_raw: Some(7_000_000_000),
            min_ram_gb: 6.0,
            recommended_ram_gb: 12.0,
            min_vram_gb: Some(6.0),
            quantization: "Q4_K_M".to_string(),
            context_length: 8_192,
            use_case: "Coding".to_string(),
        }
    }

    #[test]
    fn named_model_fit_reports_capability_without_selecting_a_model() {
        let receipt = fit(&seven_b_model(), &cpu_machine(24.0)).expect("fit receipt");

        assert_eq!(receipt.model_name, "Fixture-7B");
        assert_eq!(receipt.run_mode, RunMode::CpuOnly);
        assert_ne!(receipt.fit_level, FitLevel::TooTight);
        assert!(receipt.memory_required_gb > 0.0);
        assert!(receipt.estimated_tokens_per_second > 0.0);
        assert!(receipt.estimate_notice.contains("Estimate-based"));
    }

    #[test]
    fn named_model_fit_fails_closed_when_memory_is_insufficient() {
        let receipt = fit(&seven_b_model(), &cpu_machine(1.0)).expect("fit receipt");

        assert_eq!(receipt.model_name, "Fixture-7B");
        assert_eq!(receipt.fit_level, FitLevel::TooTight);
        assert!(receipt.memory_required_gb > receipt.memory_available_gb);
    }

    #[test]
    fn bundled_model_lookup_requires_the_callers_exact_name() {
        let exact = named_model("Fu01978/Nano-H").expect("exact bundled model");

        assert_eq!(exact.name, "Fu01978/Nano-H");
        assert!(matches!(
            named_model("Nano-H"),
            Err(FitError::UnknownModel(_))
        ));
    }

    #[test]
    fn unified_memory_fit_uses_the_measured_metal_limit_not_total_ram() {
        let machine = Machine::from_specs(SystemSpecs {
            total_ram_gb: 128.0,
            available_ram_gb: 120.0,
            total_cpu_cores: 18,
            cpu_name: "Apple Fixture".to_string(),
            has_gpu: true,
            gpu_vram_gb: Some(128.0),
            total_gpu_vram_gb: Some(128.0),
            gpu_available_gb: Some(90.0),
            gpu_name: Some("Apple Fixture".to_string()),
            gpu_count: 1,
            unified_memory: true,
            backend: GpuBackend::Metal,
            gpus: Vec::new(),
            cluster_mode: false,
            cluster_node_count: 0,
        });
        let model = Model {
            name: "Fixture-200B".to_string(),
            provider: "fixture".to_string(),
            parameter_count: "200B".to_string(),
            parameters_raw: Some(200_000_000_000),
            min_ram_gb: 100.0,
            recommended_ram_gb: 120.0,
            min_vram_gb: Some(100.0),
            quantization: "Q4_K_M".to_string(),
            context_length: 8_192,
            use_case: "Coding".to_string(),
        };

        let receipt = fit(&model, &machine).expect("fit receipt");
        assert_eq!(receipt.fit_level, FitLevel::TooTight);
        assert_eq!(receipt.memory_available_gb, 90.0);
        assert!(machine.summary_line().contains("90.0 GB Metal limit"));
    }
}
