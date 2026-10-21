//! Host window preferences, independent of web entries and their browser data.
use crate::{read_json, unique_id, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub version: u32,
    /// Normal inner size in logical pixels; position is the outer physical origin.
    pub width: f64,
    pub height: f64,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
}

#[derive(Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl WorkArea {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let dx = i64::from(x) - i64::from(self.x);
        let dy = i64::from(y) - i64::from(self.y);
        dx >= 0 && dy >= 0 && dx < i64::from(self.width) && dy < i64::from(self.height)
    }
}

impl WindowState {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err("窗口状态格式或版本无效，原文件已保留。".into());
        }
        Ok(())
    }

    /// Fit the complete frame to the selected monitor, including its taskbar.
    /// Insets and minimum size are logical pixels, allowing a DPI change on restore.
    pub fn fit(&mut self, area: WorkArea, insets: (f64, f64), minimum: (f64, f64), center: bool) {
        let available_width = (f64::from(area.width) / area.scale - insets.0).max(1.0);
        let available_height = (f64::from(area.height) / area.scale - insets.1).max(1.0);
        self.width = self
            .width
            .clamp(minimum.0.min(available_width), available_width);
        self.height = self
            .height
            .clamp(minimum.1.min(available_height), available_height);
        let remaining_x = (f64::from(area.width) - (self.width + insets.0) * area.scale).max(0.0);
        let remaining_y = (f64::from(area.height) - (self.height + insets.1) * area.scale).max(0.0);
        self.x = (f64::from(area.x)
            + if center {
                remaining_x / 2.0
            } else {
                (f64::from(self.x) - f64::from(area.x)).clamp(0.0, remaining_x)
            })
        .round() as i32;
        self.y = (f64::from(area.y)
            + if center {
                remaining_y / 2.0
            } else {
                (f64::from(self.y) - f64::from(area.y)).clamp(0.0, remaining_y)
            })
        .round() as i32;
    }
}

pub fn load(path: &Path) -> Result<Option<WindowState>> {
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let state: WindowState = serde_json::from_str(&read_json(path)?)
        .map_err(|_| "窗口状态格式无效，原文件已保留。".to_owned())?;
    state.validate()?;
    Ok(Some(state))
}

pub fn save(path: &Path, state: &WindowState) -> Result<()> {
    state.validate()?;
    let parent = path.parent().ok_or("窗口状态目录无效。")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let pending = parent.join(format!("window-state-{}.tmp", unique_id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&pending, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> WindowState {
        WindowState {
            version: 1,
            width: 1100.0,
            height: 760.0,
            x: 100,
            y: 80,
            maximized: false,
        }
    }

    #[test]
    fn fits_work_area_after_dpi_change_including_frame() {
        let mut value = state();
        value.fit(
            WorkArea {
                x: 0,
                y: 0,
                width: 1366,
                height: 720,
                scale: 1.5,
            },
            (10.0, 6.0),
            (480.0, 600.0),
            false,
        );
        assert!(value.width * 1.5 + 15.0 <= 1366.0);
        assert!(value.height * 1.5 + 9.0 <= 720.0);
        assert_eq!((value.x, value.y), (0, 0));
    }

    #[test]
    fn preserves_negative_monitor_coordinates_and_clamps_edges() {
        let area = WorkArea {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1040,
            scale: 1.0,
        };
        assert!(area.contains(-1900, 0));
        assert!(!area.contains(0, 0));
        let mut value = state();
        value.x = -1200;
        value.y = 500;
        value.fit(area, (16.0, 8.0), (480.0, 600.0), false);
        assert_eq!((value.x, value.y), (-1200, 72));
    }

    #[test]
    fn recenters_when_the_original_monitor_is_missing() {
        let mut value = state();
        value.x = 4000;
        value.fit(
            WorkArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
                scale: 1.0,
            },
            (0.0, 0.0),
            (480.0, 600.0),
            true,
        );
        assert_eq!((value.x, value.y), (410, 140));
    }

    #[test]
    fn state_round_trips_and_invalid_files_are_preserved() {
        let directory = std::env::temp_dir().join(format!("oriel-window-{}", unique_id()));
        let path = directory.join("window-state.json");
        assert_eq!(load(&path).unwrap(), None);
        let mut value = state();
        value.maximized = true;
        save(&path, &value).unwrap();
        assert_eq!(load(&path).unwrap(), Some(value.clone()));
        value.width = 900.0;
        save(&path, &value).unwrap();
        assert_eq!(load(&path).unwrap(), Some(value));
        for invalid in [
            "broken",
            r#"{"version":2,"width":900,"height":700,"x":0,"y":0,"maximized":false}"#,
            r#"{"version":1,"width":0,"height":700,"x":0,"y":0,"maximized":false}"#,
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
