use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Presentation only. A theme belongs to a view, never to a Document.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// An sRGB colour. Convert RGB to linear values before writing GPU instances;
/// alpha remains linear. CSS uses the same original sRGB components.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColor {
    srgb: [u8; 3],
    linear: [f32; 4],
}
impl ThemeColor {
    fn new(srgb: [u8; 3]) -> Self {
        let [r, g, b] = srgb.map(|v| {
            let srgb = f32::from(v) / 255.0;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        });
        Self {
            srgb,
            linear: [r, g, b, 1.0],
        }
    }
    /// Conversion is cached per palette, so drawing many nodes only copies values.
    pub fn linear(self) -> [f32; 4] {
        self.linear
    }
    pub fn css(self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            self.srgb[0], self.srgb[1], self.srgb[2]
        )
    }
}

/// Shared tokens for Rust/GPU and WebView controls.
pub struct ThemePalette {
    pub shadow: ThemeColor,
    pub mint: ThemeColor,
    pub violet: ThemeColor,
    pub amber: ThemeColor,
    pub rose: ThemeColor,
    pub background: ThemeColor,
    pub grid: ThemeColor,
    pub node: ThemeColor,
    pub border: ThemeColor,
    pub accent: ThemeColor,
    pub edge: ThemeColor,
    pub port: ThemeColor,
    pub text: ThemeColor,
    pub muted: ThemeColor,
    pub hover: ThemeColor,
    pub cable_valid: ThemeColor,
    pub cable_invalid: ThemeColor,
}
impl Theme {
    pub fn palette(self) -> ThemePalette {
        match self {
            Self::Dark => ThemePalette {
                shadow: ThemeColor::new([0, 0, 0]),
                mint: ThemeColor::new([95, 213, 184]),
                violet: ThemeColor::new([182, 157, 249]),
                amber: ThemeColor::new([241, 189, 104]),
                rose: ThemeColor::new([238, 139, 160]),
                background: ThemeColor::new([20, 22, 28]),
                grid: ThemeColor::new([55, 61, 74]),
                node: ThemeColor::new([33, 37, 47]),
                border: ThemeColor::new([107, 120, 140]),
                accent: ThemeColor::new([120, 162, 255]),
                edge: ThemeColor::new([133, 153, 190]),
                port: ThemeColor::new([120, 162, 255]),
                text: ThemeColor::new([234, 238, 246]),
                muted: ThemeColor::new([169, 181, 202]),
                hover: ThemeColor::new([43, 49, 62]),
                cable_valid: ThemeColor::new([120, 162, 255]),
                cable_invalid: ThemeColor::new([244, 158, 95]),
            },
            Self::Light => ThemePalette {
                shadow: ThemeColor::new([76, 91, 117]),
                mint: ThemeColor::new([20, 128, 107]),
                violet: ThemeColor::new([126, 84, 192]),
                amber: ThemeColor::new([161, 105, 23]),
                rose: ThemeColor::new([183, 66, 103]),
                background: ThemeColor::new([245, 247, 250]),
                grid: ThemeColor::new([185, 196, 211]),
                node: ThemeColor::new([255, 255, 255]),
                border: ThemeColor::new([126, 141, 161]),
                accent: ThemeColor::new([45, 99, 213]),
                edge: ThemeColor::new([84, 112, 157]),
                port: ThemeColor::new([45, 99, 213]),
                text: ThemeColor::new([27, 43, 57]),
                muted: ThemeColor::new([76, 95, 112]),
                hover: ThemeColor::new([228, 237, 242]),
                cable_valid: ThemeColor::new([45, 99, 213]),
                cable_invalid: ThemeColor::new([174, 69, 22]),
            },
        }
    }
}
impl ThemePalette {
    /// Small presentation metadata, suitable for IPC. Keys are CSS token names.
    pub fn css_variables(&self) -> BTreeMap<String, String> {
        [
            ("background", self.background),
            ("surface", self.node),
            ("border", self.border),
            ("accent", self.accent),
            ("text", self.text),
            ("muted", self.muted),
            ("hover", self.hover),
        ]
        .into_iter()
        .map(|(key, color)| (key.into(), color.css()))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contrast(a: ThemeColor, b: ThemeColor) -> f32 {
        let luminance = |c: ThemeColor| {
            let [r, g, b, _] = c.linear();
            0.2126 * r + 0.7152 * g + 0.0722 * b
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn text_and_interaction_tokens_remain_readable_in_both_themes() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = theme.palette();
            for surface in [p.background, p.node, p.hover] {
                assert!(contrast(p.text, surface) >= 4.5);
                assert!(contrast(p.muted, surface) >= 4.5);
                assert!(contrast(p.accent, surface) >= 3.0);
            }
            assert!(contrast(p.border, p.node) >= 3.0);
            assert!(contrast(p.edge, p.background) >= 3.0);
        }
    }
    #[test]
    fn css_and_gpu_share_srgb_tokens_with_linear_gpu_values() {
        let c = ThemeColor::new([128, 0, 255]);
        assert_eq!(c.css(), "#8000ff");
        let linear = c.linear();
        assert!((linear[0] - 0.21586).abs() < 0.00001);
        assert_eq!(&linear[1..], &[0., 1., 1.]);
    }
}

/// Host-selected visual role. Presentation metadata only; no application logic in core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NodeTone {
    #[default]
    Neutral,
    Blue,
    Mint,
    Violet,
    Amber,
    Rose,
}
impl ThemePalette {
    pub fn tone(&self, tone: NodeTone) -> ThemeColor {
        match tone {
            NodeTone::Neutral => self.border,
            NodeTone::Blue => self.accent,
            NodeTone::Mint => self.mint,
            NodeTone::Violet => self.violet,
            NodeTone::Amber => self.amber,
            NodeTone::Rose => self.rose,
        }
    }
}
