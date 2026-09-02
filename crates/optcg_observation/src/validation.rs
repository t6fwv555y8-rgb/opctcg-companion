use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationStatus {
    Complete,
    Partial,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureTestStatus {
    Pass,
    Fail,
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiveValidationStatus {
    Required,
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterValidationStatus {
    pub adapter: String,
    pub implementation: ImplementationStatus,
    pub fixture_tests: FixtureTestStatus,
    pub live_validation: LiveValidationStatus,
}

pub fn onesimulator_validation() -> AdapterValidationStatus {
    AdapterValidationStatus {
        adapter: "OneSimulator".into(),
        implementation: ImplementationStatus::Complete,
        fixture_tests: FixtureTestStatus::Pass,
        live_validation: LiveValidationStatus::Required,
    }
}

pub fn optcgsim_validation() -> AdapterValidationStatus {
    AdapterValidationStatus {
        adapter: "OPTCGSim".into(),
        implementation: ImplementationStatus::Complete,
        fixture_tests: FixtureTestStatus::Pass,
        live_validation: LiveValidationStatus::Required,
    }
}

pub fn all_adapter_validation() -> Vec<AdapterValidationStatus> {
    vec![onesimulator_validation(), optcgsim_validation()]
}
