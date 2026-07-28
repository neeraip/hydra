//! Document assembly (spec §3): pair a template with a producer to build a
//! render-ready document. Assembly cannot fail — block production failures
//! become explicit placeholder sections, never silent omissions.

use hydra_common::{BlockError, Fragment};

use crate::template::ReportTemplate;

/// Caller-supplied provenance (spec §3). This layer never reads the clock —
/// identical inputs must yield byte-identical output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportContext {
    /// RFC 3339 generation timestamp text; omitted when the caller wants
    /// fully reproducible output.
    pub generated_at: Option<String>,
    /// Ordered (label, value) source pairs — project name, scenario, file…
    pub source: Vec<(String, String)>,
}

/// One rendered section of a document (spec §3), in template order.
#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    /// The produced fragment, heading override already applied.
    Content(Fragment),
    /// The block does not apply to this run (engine-authored reason).
    Unavailable { title: String, reason: String },
    /// Production failed, including unknown block ids.
    Failed { title: String, message: String },
}

impl Section {
    /// The section heading, whatever the variant.
    pub fn title(&self) -> &str {
        match self {
            Section::Content(fragment) => &fragment.title,
            Section::Unavailable { title, .. } | Section::Failed { title, .. } => title,
        }
    }
}

/// A render-ready document (spec §3).
#[derive(Debug, Clone, PartialEq)]
pub struct ReportDocument {
    pub title: String,
    pub generated_at: Option<String>,
    pub source: Vec<(String, String)>,
    pub sections: Vec<Section>,
}

/// Assemble a document from a template and a producer — the function the
/// application supplies to map a block id (plus the block's optional,
/// opaque options value) to a fragment or block error (an engine's
/// `produce_report_block` behind the scenes). Placeholder sections use
/// the heading override when present, falling back to the block id (the
/// engine's default heading is unavailable on failure).
pub fn assemble(
    template: &ReportTemplate,
    context: ReportContext,
    mut produce: impl FnMut(&str, Option<&serde_json::Value>) -> Result<Fragment, BlockError>,
) -> ReportDocument {
    let sections = template
        .blocks
        .iter()
        .map(|block| {
            let placeholder_title = || block.title.clone().unwrap_or_else(|| block.id.clone());
            match produce(&block.id, block.options.as_ref()) {
                Ok(mut fragment) => {
                    if let Some(title) = &block.title {
                        fragment.title = title.clone();
                    }
                    Section::Content(fragment)
                }
                Err(BlockError::Unavailable { reason }) => Section::Unavailable {
                    title: placeholder_title(),
                    reason,
                },
                Err(err) => Section::Failed {
                    title: placeholder_title(),
                    message: err.to_string(),
                },
            }
        })
        .collect();

    ReportDocument {
        title: template.title.clone(),
        generated_at: context.generated_at,
        source: context.source,
        sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateBlock;
    use hydra_common::FragmentItem;

    fn template(blocks: Vec<TemplateBlock>) -> ReportTemplate {
        ReportTemplate {
            version: 1,
            title: "Doc".into(),
            blocks,
        }
    }

    fn fragment(title: &str) -> Fragment {
        Fragment {
            title: title.into(),
            items: vec![FragmentItem::Note {
                text: "note".into(),
            }],
        }
    }

    #[test]
    fn maps_blocks_to_sections_in_template_order() {
        let t = template(vec![
            TemplateBlock {
                id: "e.ok".into(),
                title: None,
                options: None,
            },
            TemplateBlock {
                id: "e.gone".into(),
                title: None,
                options: None,
            },
            TemplateBlock {
                id: "e.bad".into(),
                title: Some("Renamed".into()),
                options: None,
            },
        ]);
        let doc = assemble(&t, ReportContext::default(), |id, _options| match id {
            "e.ok" => Ok(fragment("OK")),
            "e.gone" => Err(BlockError::Unavailable {
                reason: "no pumps".into(),
            }),
            _ => Err(BlockError::UnknownBlock { id: id.into() }),
        });
        assert_eq!(doc.sections.len(), 3);
        assert_eq!(doc.sections[0].title(), "OK");
        assert!(matches!(
            &doc.sections[1],
            Section::Unavailable { title, reason } if title == "e.gone" && reason == "no pumps"
        ));
        assert!(matches!(
            &doc.sections[2],
            Section::Failed { title, message } if title == "Renamed" && message.contains("e.bad")
        ));
    }

    #[test]
    fn heading_override_applies_to_content() {
        let t = template(vec![TemplateBlock {
            id: "e.ok".into(),
            title: Some("Custom".into()),
            options: None,
        }]);
        let doc = assemble(&t, ReportContext::default(), |_, _| Ok(fragment("Default")));
        assert_eq!(doc.sections[0].title(), "Custom");
    }

    #[test]
    fn passes_block_options_to_the_producer_verbatim() {
        let t = template(vec![TemplateBlock {
            id: "e.opt".into(),
            title: None,
            options: Some(serde_json::json!({ "minPressure": 20 })),
        }]);
        let doc = assemble(&t, ReportContext::default(), |_, options| {
            let min = options
                .and_then(|o| o.get("minPressure"))
                .and_then(|v| v.as_i64());
            assert_eq!(min, Some(20));
            Ok(fragment("With options"))
        });
        assert_eq!(doc.sections[0].title(), "With options");
    }

    #[test]
    fn carries_context_verbatim() {
        let t = template(vec![]);
        let doc = assemble(
            &t,
            ReportContext {
                generated_at: Some("2026-07-28T00:00:00Z".into()),
                source: vec![("Project".into(), "Anytown".into())],
            },
            |_, _| unreachable!("no blocks"),
        );
        assert_eq!(doc.generated_at.as_deref(), Some("2026-07-28T00:00:00Z"));
        assert_eq!(doc.source[0].1, "Anytown");
        assert!(doc.sections.is_empty());
    }
}
