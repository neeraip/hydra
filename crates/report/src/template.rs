//! Report templates (spec §2): the user's saved answer to "what goes in
//! my report" — a JSON document shared verbatim between the GUI's template
//! builder and headless CLI generation.

use hydra_common::BlockDescriptor;
use serde::{Deserialize, Serialize};

/// One block reference in a template (spec §2). The id is opaque to this
/// layer — it is validated only by the producing engine at assembly time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateBlock {
    pub id: String,
    /// Optional heading override replacing the block's default heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional per-block options, passed to the producing engine verbatim
    /// (hydra-common spec §3.4) — fully opaque to this layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// A report template (spec §2). Unknown fields are ignored on read
/// (additive evolution); breaking format changes require a version bump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportTemplate {
    /// Template format version; must equal [`ReportTemplate::VERSION`].
    pub version: u32,
    /// Document title. Non-empty.
    pub title: String,
    /// Ordered block references; empty is valid (a document with no
    /// sections).
    #[serde(default)]
    pub blocks: Vec<TemplateBlock>,
}

/// Template read failure (spec §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The bytes are not valid JSON for the template shape.
    Json { message: String },
    /// The template declares a format version this build does not read.
    UnsupportedVersion { version: u32 },
    /// The title is empty or whitespace-only.
    EmptyTitle,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Json { message } => write!(f, "invalid template JSON: {message}"),
            TemplateError::UnsupportedVersion { version } => write!(
                f,
                "unsupported template version {version} (this build reads version {})",
                ReportTemplate::VERSION
            ),
            TemplateError::EmptyTitle => write!(f, "template title must not be empty"),
        }
    }
}

impl std::error::Error for TemplateError {}

impl ReportTemplate {
    /// The template format version this build reads and writes.
    pub const VERSION: u32 = 1;

    /// Parse and validate a template from JSON text (spec §2).
    pub fn from_json(json: &str) -> Result<Self, TemplateError> {
        let template: ReportTemplate =
            serde_json::from_str(json).map_err(|e| TemplateError::Json {
                message: e.to_string(),
            })?;
        if template.version != Self::VERSION {
            return Err(TemplateError::UnsupportedVersion {
                version: template.version,
            });
        }
        if template.title.trim().is_empty() {
            return Err(TemplateError::EmptyTitle);
        }
        Ok(template)
    }

    /// Serialise to pretty-printed JSON (the on-disk template format).
    pub fn to_json(&self) -> String {
        // Serialisation of this shape cannot fail; fall back to an empty
        // object rather than panicking in a release path.
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// A template covering every block of a catalog in catalog order, with
    /// no heading overrides — the "everything report" default.
    pub fn covering(title: &str, catalog: &[BlockDescriptor]) -> Self {
        ReportTemplate {
            version: Self::VERSION,
            title: title.into(),
            blocks: catalog
                .iter()
                .map(|b| TemplateBlock {
                    id: b.id.into(),
                    title: None,
                    options: None,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_template() {
        let t = ReportTemplate::from_json(
            r#"{ "version": 1, "title": "Q3", "blocks": [{ "id": "wds.run-summary" }] }"#,
        )
        .unwrap();
        assert_eq!(t.title, "Q3");
        assert_eq!(t.blocks[0].id, "wds.run-summary");
        assert_eq!(t.blocks[0].title, None);
    }

    #[test]
    fn ignores_unknown_fields_and_allows_empty_blocks() {
        let t = ReportTemplate::from_json(
            r#"{ "version": 1, "title": "T", "future": true, "blocks": [] }"#,
        )
        .unwrap();
        assert!(t.blocks.is_empty());
    }

    #[test]
    fn rejects_bad_version_bad_json_and_empty_title() {
        assert_eq!(
            ReportTemplate::from_json(r#"{ "version": 2, "title": "T" }"#),
            Err(TemplateError::UnsupportedVersion { version: 2 })
        );
        assert!(matches!(
            ReportTemplate::from_json("not json"),
            Err(TemplateError::Json { .. })
        ));
        assert_eq!(
            ReportTemplate::from_json(r#"{ "version": 1, "title": "  " }"#),
            Err(TemplateError::EmptyTitle)
        );
    }

    #[test]
    fn json_round_trips() {
        let t = ReportTemplate {
            version: 1,
            title: "T".into(),
            blocks: vec![TemplateBlock {
                id: "wds.x".into(),
                title: Some("Override".into()),
                options: Some(serde_json::json!({ "minPressure": 20 })),
            }],
        };
        assert_eq!(ReportTemplate::from_json(&t.to_json()).unwrap(), t);
    }

    #[test]
    fn covering_takes_catalog_order() {
        let catalog = [
            BlockDescriptor {
                id: "e.one",
                title: "One",
                summary: "",
                category: "General",
            },
            BlockDescriptor {
                id: "e.two",
                title: "Two",
                summary: "",
                category: "General",
            },
        ];
        let t = ReportTemplate::covering("All", &catalog);
        let ids: Vec<_> = t.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["e.one", "e.two"]);
    }
}
