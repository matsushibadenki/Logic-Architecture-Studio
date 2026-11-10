use std::collections::BTreeMap;
use unge_core::{Locale, Rect};

/// Presentation metadata supplied by the host; never persisted in Document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelText {
    pub en: String,
    pub ja: String,
    pub zh_cn: String,
}
impl LabelText {
    pub fn get(&self, locale: Locale) -> &str {
        match locale {
            Locale::En => &self.en,
            Locale::Ja => &self.ja,
            Locale::ZhCn => &self.zh_cn,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeLabels {
    /// Accent shared by the header, ports and outgoing cables.
    pub tone: crate::NodeTone,
    /// Optional short symbol (two characters maximum) and one-line footer.
    pub symbol: String,
    pub caption: LabelText,
    pub title: LabelText,
    /// Keys are stable port identifiers; translations never change connections.
    pub inputs: BTreeMap<String, LabelText>,
    pub outputs: BTreeMap<String, LabelText>,
}
pub type LabelCatalog = BTreeMap<String, NodeLabels>;

/// A single, clipped line in world coordinates. Labels are painted after the
/// indicated geometry instance, preserving node stacking and preview overlays.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLabel {
    pub text: String,
    pub rect: Rect,
    pub font_size: f32,
    pub right_aligned: bool,
    pub color: [f32; 4],
    pub after_quad: usize,
}
pub(crate) fn label_text(localized: Option<&LabelText>, fallback: &str, locale: Locale) -> String {
    let text = localized
        .map(|v| v.get(locale))
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    // Bound shaping work independently of host metadata length. Keep one line.
    text.chars()
        .take(256)
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}
