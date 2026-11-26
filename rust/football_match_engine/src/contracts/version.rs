//! Unified `VersionManifest4` and simulation compatibility hashing (Spec Section 8.1).
//!
//! Separates build traceability, wire/checkpoint schemas, gameplay behavior identity,
//! match configuration identity, and Android presentation identity so presentation upgrades
//! never invalidate Rust simulation checkpoints.

use serde::{Deserialize, Serialize};

use crate::fingerprints;
use crate::player_intelligence::{PI_MODULE_VERSION, PI_POLICY_ID};
use crate::v4::{
    CHECKPOINT_SCHEMA_VERSION_V4, DIAGNOSTICS_SCHEMA_VERSION_V4, ENGINE_PROTOCOL_VERSION_V4,
    ENGINE_VERSION_V4, EVENT_SCHEMA_VERSION_V4, JNI_BRIDGE_VERSION,
};

pub const VERSION_MANIFEST_SCHEMA: u32 = 1;
pub const CANONICAL_FINGERPRINT_VERSION: u32 = 1;
pub const RNG_ALGORITHM_ID: &str = "splitmix64-java-v317-compat";
pub const NUMERIC_POLICY_ID: &str = "ieee754-f32-f64-float-roundtrip";

/// Traceability metadata for the compiled binary; excluded from `compatibility_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildIdentity {
    pub crate_name: String,
    pub crate_version: String,
    pub jni_bridge_version: i32,
    pub target_arch: String,
    pub target_os: String,
}

/// Wire, event, checkpoint, and telemetry schema versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSchemas {
    pub manifest_schema: u32,
    pub protocol_version: u32,
    pub event_schema_version: u32,
    pub checkpoint_schema_version: u32,
    pub diagnostics_schema_version: u32,
    pub canonical_fingerprint_version: u32,
}

/// Behavior identity for each gameplay subsystem; changes here alter `compatibility_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplayIdentity {
    pub engine_version: String,
    pub physics_version: String,
    pub rules_version: String,
    pub condition_version: String,
    pub tactics_version: String,
    pub pi_module_version: String,
    pub pi_policy_id: String,
    pub coach_version: String,
    pub rng_algorithm: String,
    pub numeric_policy: String,
}

/// Optional match-specific configuration identity for checkpoints and replays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchConfigIdentity {
    pub match_id: String,
    pub seed: i64,
    pub ruleset_id: String,
    pub ruleset_version: u32,
    pub input_fingerprint: String,
}

/// Optional Android presentation identity; strictly excluded from simulation `compatibility_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationIdentity {
    pub playback_version: String,
    pub commentary_version: String,
    pub audio_version: String,
    pub locale: String,
    pub viewing_mode: String,
}

/// Canonical owned version manifest for Football Simulation 4.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest4 {
    pub build: BuildIdentity,
    pub schemas: DataSchemas,
    pub gameplay: GameplayIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<MatchConfigIdentity>,
    pub compatibility_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<PresentationIdentity>,
}

impl VersionManifest4 {
    /// Builds the manifest for the currently compiled Match Engine 4.x runtime.
    pub fn current_engine_manifest() -> Self {
        let schemas = DataSchemas {
            manifest_schema: VERSION_MANIFEST_SCHEMA,
            protocol_version: ENGINE_PROTOCOL_VERSION_V4,
            event_schema_version: EVENT_SCHEMA_VERSION_V4,
            checkpoint_schema_version: CHECKPOINT_SCHEMA_VERSION_V4,
            diagnostics_schema_version: DIAGNOSTICS_SCHEMA_VERSION_V4,
            canonical_fingerprint_version: CANONICAL_FINGERPRINT_VERSION,
        };
        let gameplay = GameplayIdentity {
            engine_version: ENGINE_VERSION_V4.to_string(),
            physics_version: "legacy-des-pi4.0.28".to_string(),
            rules_version: "legacy-des-pi4.0.28".to_string(),
            condition_version: "legacy-des-pi4.0.28".to_string(),
            tactics_version: "legacy-des-pi4.0.28".to_string(),
            pi_module_version: PI_MODULE_VERSION.to_string(),
            pi_policy_id: PI_POLICY_ID.to_string(),
            coach_version: "legacy-des-pi4.0.28".to_string(),
            rng_algorithm: RNG_ALGORITHM_ID.to_string(),
            numeric_policy: NUMERIC_POLICY_ID.to_string(),
        };
        let compatibility_id = Self::compute_compatibility_id(&schemas, &gameplay);
        Self {
            build: BuildIdentity {
                crate_name: env!("CARGO_PKG_NAME").to_string(),
                crate_version: env!("CARGO_PKG_VERSION").to_string(),
                jni_bridge_version: JNI_BRIDGE_VERSION,
                target_arch: std::env::consts::ARCH.to_string(),
                target_os: std::env::consts::OS.to_string(),
            },
            schemas,
            gameplay,
            config: None,
            compatibility_id,
            presentation: None,
        }
    }

    /// Computes a deterministic compatibility digest strictly from simulation schemas
    /// and gameplay behavior identity (never build path, OS, or Android presentation version).
    pub fn compute_compatibility_id(schemas: &DataSchemas, gameplay: &GameplayIdentity) -> String {
        let mut h = fingerprints::start();
        h = fingerprints::mix_i32(h, schemas.manifest_schema as i32);
        h = fingerprints::mix_i32(h, schemas.protocol_version as i32);
        h = fingerprints::mix_i32(h, schemas.event_schema_version as i32);
        h = fingerprints::mix_i32(h, schemas.checkpoint_schema_version as i32);
        h = fingerprints::mix_i32(h, schemas.diagnostics_schema_version as i32);
        h = fingerprints::mix_i32(h, schemas.canonical_fingerprint_version as i32);
        h = fingerprints::mix_string(h, Some(&gameplay.engine_version));
        h = fingerprints::mix_string(h, Some(&gameplay.physics_version));
        h = fingerprints::mix_string(h, Some(&gameplay.rules_version));
        h = fingerprints::mix_string(h, Some(&gameplay.condition_version));
        h = fingerprints::mix_string(h, Some(&gameplay.tactics_version));
        h = fingerprints::mix_string(h, Some(&gameplay.pi_module_version));
        h = fingerprints::mix_string(h, Some(&gameplay.pi_policy_id));
        h = fingerprints::mix_string(h, Some(&gameplay.coach_version));
        h = fingerprints::mix_string(h, Some(&gameplay.rng_algorithm));
        h = fingerprints::mix_string(h, Some(&gameplay.numeric_policy));
        format!("sim4-{h:016x}")
    }

    /// Returns a copy with attached match configuration identity without altering `compatibility_id`.
    pub fn with_config(mut self, config: MatchConfigIdentity) -> Self {
        self.config = Some(config);
        self
    }

    /// Returns a copy with attached presentation identity without altering `compatibility_id`.
    pub fn with_presentation(mut self, presentation: PresentationIdentity) -> Self {
        self.presentation = Some(presentation);
        self
    }

    /// Match-level continuation additionally requires identical normalized input,
    /// seed and ruleset; build and presentation identities are not match identity.
    pub fn check_match_compatibility(&self, other: &Self) -> Result<(), String> {
        self.check_simulation_compatibility(other)?;
        match (&self.config, &other.config) {
            (Some(a), Some(b)) if a == b => Ok(()),
            (Some(_), Some(_)) => Err("Incompatible match configuration".into()),
            _ => Err("Missing match configuration".into()),
        }
    }

    /// Verifies that another manifest is simulation-compatible for checkpoint/replay continuation.
    pub fn check_simulation_compatibility(&self, other: &Self) -> Result<(), String> {
        if self.schemas.manifest_schema != VERSION_MANIFEST_SCHEMA || other.schemas.manifest_schema != VERSION_MANIFEST_SCHEMA { return Err("Unsupported manifest schema".into()); }
        let expected_self = Self::compute_compatibility_id(&self.schemas, &self.gameplay);
        if self.compatibility_id != expected_self {
            return Err(format!(
                "Local manifest compatibility_id mismatch: stored={} computed={}",
                self.compatibility_id, expected_self
            ));
        }
        let expected_other = Self::compute_compatibility_id(&other.schemas, &other.gameplay);
        if other.compatibility_id != expected_other {
            return Err(format!(
                "Candidate manifest compatibility_id tampered/invalid: stored={} computed={}",
                other.compatibility_id, expected_other
            ));
        }
        if self.schemas.checkpoint_schema_version != other.schemas.checkpoint_schema_version {
            return Err(format!(
                "Incompatible checkpoint schema: expected={} got={}",
                self.schemas.checkpoint_schema_version, other.schemas.checkpoint_schema_version
            ));
        }
        if self.compatibility_id != other.compatibility_id {
            return Err(format!(
                "Incompatible simulation behavior: expected={} (pi={}) got={} (pi={})",
                self.compatibility_id,
                self.gameplay.pi_module_version,
                other.compatibility_id,
                other.gameplay.pi_module_version
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_manifest_roundtrip_and_presentation_independence() {
        let base = VersionManifest4::current_engine_manifest();
        let json = serde_json::to_string(&base).expect("serialize manifest");
        let parsed: VersionManifest4 = serde_json::from_str(&json).expect("deserialize manifest");
        assert_eq!(base, parsed);
        assert!(base.check_simulation_compatibility(&parsed).is_ok());

        // Changing presentation version must NOT invalidate simulation compatibility.
        let with_pres = base.clone().with_presentation(PresentationIdentity {
            playback_version: "4.2.0".to_string(),
            commentary_version: "4.5.1".to_string(),
            audio_version: "4.1.0".to_string(),
            locale: "vi".to_string(),
            viewing_mode: "FULL_MATCH".to_string(),
        });
        assert_eq!(base.compatibility_id, with_pres.compatibility_id);
        assert!(base.check_simulation_compatibility(&with_pres).is_ok());
    }

    #[test]
    fn version_manifest_rejects_behavior_or_schema_mismatch() {
        let base = VersionManifest4::current_engine_manifest();
        let mut changed_pi = base.clone();
        changed_pi.gameplay.pi_module_version = "4.0.29".to_string();
        changed_pi.compatibility_id =
            VersionManifest4::compute_compatibility_id(&changed_pi.schemas, &changed_pi.gameplay);
        assert_ne!(base.compatibility_id, changed_pi.compatibility_id);
        assert!(base.check_simulation_compatibility(&changed_pi).is_err());

        let mut tampered = base.clone();
        tampered.gameplay.rules_version = "4.1.0".to_string();
        // compatibility_id intentionally NOT recomputed -> must reject tampered manifest
        assert!(base.check_simulation_compatibility(&tampered).is_err());
    }
}

#[cfg(test)]
mod a2_contract_tests {
    use super::*;

    #[test]
    fn manifest_schema_build_and_match_configuration_contract() {
        let base = VersionManifest4::current_engine_manifest();
        let mut changed = base.clone();
        changed.build.target_arch = "another-target".into();
        changed.build.crate_version = "another-build".into();
        assert!(base.check_simulation_compatibility(&changed).is_ok());

        changed.schemas.manifest_schema += 1;
        changed.compatibility_id = VersionManifest4::compute_compatibility_id(
            &changed.schemas, &changed.gameplay);
        assert!(base.check_simulation_compatibility(&changed).is_err());

        changed = base.clone();
        changed.schemas.checkpoint_schema_version += 1;
        changed.compatibility_id = VersionManifest4::compute_compatibility_id(
            &changed.schemas, &changed.gameplay);
        assert!(base.check_simulation_compatibility(&changed).is_err());
        assert!(serde_json::from_str::<VersionManifest4>("{\"schemas\":{}}").is_err());

        let config = MatchConfigIdentity {
            match_id: "fixture-a2".into(), seed: 42, ruleset_id: "ifab_league_standard".into(),
            ruleset_version: 1, input_fingerprint: "fixed-input".into(),
        };
        let a = base.clone().with_config(config.clone());
        assert!(a.check_match_compatibility(&a).is_ok());
        assert!(a.check_match_compatibility(&base).is_err());
        let mut wrong = config;
        wrong.seed += 1;
        let b = base.with_config(wrong);
        assert!(a.check_simulation_compatibility(&b).is_ok());
        assert!(a.check_match_compatibility(&b).is_err());
    }
}
