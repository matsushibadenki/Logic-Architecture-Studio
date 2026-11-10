use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::Manager;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub schema: u32,
    pub sizes: [f64; 3],
    pub homes: BTreeMap<String, String>,
    pub frames: BTreeMap<String, Frame>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            schema: 1,
            sizes: [300., 300., 230.],
            homes: BTreeMap::new(),
            frames: BTreeMap::new(),
        }
    }
}
impl Saved {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self
                .sizes
                .iter()
                .enumerate()
                .any(|(i, v)| !v.is_finite() || *v < if i == 2 { 120. } else { 180. } || *v > 700.)
            || self.homes.iter().any(|(k, v)| {
                !super::panels::IDS.contains(&k.as_str())
                    || !["left", "right", "bottom"].contains(&v.as_str())
            })
            || self.frames.iter().any(|(k, f)| {
                !super::panels::IDS.contains(&k.as_str())
                    || !f.width.is_finite()
                    || !f.height.is_finite()
                    || f.width < 300.
                    || f.height < 250.
                    || f.width > 10000.
                    || f.height > 10000.
            })
        {
            return Err("Invalid panel layout file; original preserved".into());
        }
        Ok(())
    }
}
pub struct Store {
    pub data: Mutex<Saved>,
    path: PathBuf,
}
impl Store {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(".logic/panel-layout.json");
        let data = if path.exists() {
            let data: Saved =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("Panel layout preserved: {e}"))?;
            data.validate()?;
            data
        } else {
            Saved::default()
        };
        Ok(Self {
            data: Mutex::new(data),
            path,
        })
    }
    pub fn save(&self) -> Result<(), String> {
        let data = self.data.lock().map_err(|e| e.to_string())?;
        data.validate()?;
        let bytes = serde_json::to_vec_pretty(&*data).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("tmp");
        std::fs::create_dir_all(self.path.parent().unwrap()).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut f = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
        f.write_all(&bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(temp, &self.path).map_err(|e| e.to_string())
    }
}
pub fn save(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<Store>();
    let homes = app
        .state::<super::panels::Panels>()
        .homes
        .lock()
        .unwrap()
        .clone();
    let sizes = app
        .state::<super::panels::Gestures>()
        .sizes
        .lock()
        .unwrap()
        .unwrap_or([300., 300., 230.]);
    {
        let mut d = store.data.lock().unwrap();
        d.homes = homes;
        d.sizes = sizes;
    }
    store.save()
}
pub fn capture(app: &tauri::AppHandle, id: &str) {
    if let Some(w) = app.get_window(&format!("float-{id}")) {
        if let (Ok(p), Ok(s), Ok(scale)) = (w.outer_position(), w.inner_size(), w.scale_factor()) {
            let frame = Frame {
                x: p.x,
                y: p.y,
                width: (s.width as f64 / scale).max(300.),
                height: (s.height as f64 / scale).max(250.),
            };
            app.state::<Store>()
                .data
                .lock()
                .unwrap()
                .frames
                .insert(id.into(), frame);
        }
    }
}
pub fn capture_size(app: &tauri::AppHandle, id: &str, s: tauri::PhysicalSize<u32>) {
    if let Some(w) = app.get_window(&format!("float-{id}")) {
        if let (Ok(p), Ok(scale)) = (w.outer_position(), w.scale_factor()) {
            if s.width > 0 && s.height > 0 {
                app.state::<Store>().data.lock().unwrap().frames.insert(
                    id.into(),
                    Frame {
                        x: p.x,
                        y: p.y,
                        width: (s.width as f64 / scale).max(300.),
                        height: (s.height as f64 / scale).max(250.),
                    },
                );
            }
        }
    }
}
pub fn restore(app: &tauri::AppHandle, w: &tauri::Window, id: &str) -> Result<(), String> {
    let frame = app
        .state::<Store>()
        .data
        .lock()
        .unwrap()
        .frames
        .get(id)
        .cloned();
    if let Some(f) = frame {
        let monitors = w.available_monitors().map_err(|e| e.to_string())?;
        let monitor = monitors
            .iter()
            .find(|m| {
                let p = m.position();
                let s = m.size();
                f.x >= p.x
                    && f.y >= p.y
                    && f.x < p.x + s.width as i32
                    && f.y < p.y + s.height as i32
            })
            .or_else(|| monitors.first());
        if let Some(m) = monitor {
            let scale = m.scale_factor();
            let size = m.size();
            let width = f.width.min(size.width as f64 / scale);
            let height = f.height.min((size.height as f64 / scale - 60.).max(250.));
            w.set_size(tauri::LogicalSize::new(width, height))
                .map_err(|e| e.to_string())?;
            let p = m.position();
            let x = f.x.clamp(
                p.x,
                p.x + (size.width as i32 - (width * scale) as i32).max(0),
            );
            let y = f.y.clamp(
                p.y,
                p.y + (size.height as i32 - (height * scale) as i32 - 30).max(0),
            );
            w.set_position(tauri::PhysicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_schema_and_dimensions() {
        let mut d = Saved::default();
        d.sizes[0] = f64::NAN;
        assert!(d.validate().is_err());
        d = Saved::default();
        d.schema = 9;
        assert!(d.validate().is_err());
    }
    #[test]
    fn roundtrip_saved_layout() {
        let root = std::env::temp_dir().join(format!("las-layout-{}", std::process::id()));
        let store = Store::load(&root).unwrap();
        {
            let mut d = store.data.lock().unwrap();
            d.sizes = [375., 360., 310.];
            d.homes.insert("library".into(), "bottom".into());
            d.frames.insert(
                "inspector".into(),
                Frame {
                    x: 100,
                    y: 100,
                    width: 480.,
                    height: 600.,
                },
            );
        }
        store.save().unwrap();
        let restored = Store::load(&root).unwrap();
        let data = restored.data.lock().unwrap();
        assert_eq!(data.sizes, [375., 360., 310.]);
        assert_eq!(data.homes["library"], "bottom");
        let frame = &data.frames["inspector"];
        assert_eq!(
            (frame.x, frame.y, frame.width, frame.height),
            (100, 100, 480., 600.)
        );
        drop(data);
        std::fs::remove_dir_all(root).unwrap();
    }
}
