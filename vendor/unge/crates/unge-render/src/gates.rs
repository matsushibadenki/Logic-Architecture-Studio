//! Distinctive logic-gate outlines in a normalized 100 × 100 box.
//! All strokes are tessellated on the Rust side; no frame data crosses the IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateSymbol {
    And,
    Or,
    Xor,
    Not,
    Nand,
    Nor,
    Xnor,
}
impl GateSymbol {
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "and" => Self::And,
            "or" => Self::Or,
            "xor" => Self::Xor,
            "not" => Self::Not,
            "nand" => Self::Nand,
            "nor" => Self::Nor,
            "xnor" => Self::Xnor,
            _ => return None,
        })
    }
    pub fn inverted(self) -> bool {
        matches!(self, Self::Not | Self::Nand | Self::Nor | Self::Xnor)
    }
    pub fn input_x(self, y: f32) -> f32 {
        if matches!(self, Self::Or | Self::Nor | Self::Xor | Self::Xnor) {
            // Input boundary: quadratic (12,10), (44,50), (12,90).
            let t = (y - 10.0) / 80.0;
            12.0 + 64.0 * t * (1.0 - t)
        } else {
            12.0
        }
    }
    pub fn paths(self) -> Vec<Vec<[f32; 2]>> {
        let mut paths = match self {
            Self::And | Self::Nand => {
                let mut outline = vec![[12.0, 10.0], [46.0, 10.0]];
                for i in 1..=32 {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 32.0;
                    outline.push([46.0 + 36.0 * a.cos(), 50.0 + 40.0 * a.sin()]);
                }
                outline.extend([[12.0, 90.0], [12.0, 10.0]]);
                vec![outline]
            }
            Self::Not => vec![vec![[12.0, 10.0], [82.0, 50.0], [12.0, 90.0], [12.0, 10.0]]],
            _ => vec![
                quadratic([12.0, 10.0], [58.0, 10.0], [82.0, 50.0]),
                quadratic([82.0, 50.0], [58.0, 90.0], [12.0, 90.0]),
                quadratic([12.0, 90.0], [44.0, 50.0], [12.0, 10.0]),
            ],
        };
        if matches!(self, Self::Xor | Self::Xnor) {
            paths.push(quadratic([3.0, 10.0], [35.0, 50.0], [3.0, 90.0]));
        }
        if self.inverted() {
            paths.push(
                (0..=24)
                    .map(|i| {
                        let a = std::f32::consts::TAU * i as f32 / 24.0;
                        [88.0 + 6.0 * a.cos(), 50.0 + 6.0 * a.sin()]
                    })
                    .collect(),
            );
        }
        paths
    }
}
fn quadratic(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Vec<[f32; 2]> {
    (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0;
            let u = 1.0 - t;
            [
                u * u * a[0] + 2.0 * u * t * b[0] + t * t * c[0],
                u * u * a[1] + 2.0 * u * t * b[1] + t * t * c[1],
            ]
        })
        .collect()
}
