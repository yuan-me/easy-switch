use tauri::{LogicalSize, PhysicalPosition, PhysicalRect, PhysicalSize, WebviewWindow};

const MIN_SIZE: LogicalSize<f64> = LogicalSize::new(940.0, 620.0);

fn bounds(area: &PhysicalRect<i32, u32>, scale: f64) -> PhysicalRect<i32, u32> {
    let width = f64::from(area.size.width);
    let height = f64::from(area.size.height);
    // Match the reference proportions, while leaving margins even at high DPI.
    let size = PhysicalSize::new(
        (width * 0.65)
            .max(MIN_SIZE.width * scale)
            .min(width * 0.90)
            .round() as u32,
        (height * 0.82)
            .max(MIN_SIZE.height * scale)
            .min(height * 0.90)
            .round() as u32,
    );
    PhysicalRect {
        position: PhysicalPosition::new(
            area.position.x + ((area.size.width - size.width) / 2) as i32,
            area.position.y + ((area.size.height - size.height) / 2) as i32,
        ),
        size,
    }
}

pub fn apply(window: &WebviewWindow) -> tauri::Result<()> {
    if let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) {
        let scale = monitor.scale_factor();
        let rect = bounds(monitor.work_area(), scale);
        let logical = rect.size.to_logical::<f64>(scale);
        window.set_min_size(Some(LogicalSize::new(
            MIN_SIZE.width.min(logical.width),
            MIN_SIZE.height.min(logical.height),
        )))?;
        window.set_size(rect.size)?;
        window.set_position(rect.position)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_tracks_resolution_dpi_and_work_area_without_filling_the_screen() {
        for (width, height, scale) in [
            (1920, 1040, 1.0),
            (2560, 1392, 1.25),
            (3840, 2080, 1.5),
            (1920, 1040, 1.5),
            (1366, 728, 1.0),
            (1280, 680, 1.5),
        ] {
            let area = PhysicalRect {
                position: PhysicalPosition::new(-2560, 40),
                size: PhysicalSize::new(width, height),
            };
            let rect = bounds(&area, scale);
            assert!(rect.size.width <= (f64::from(width) * 0.9).round() as u32);
            assert!(rect.size.height <= (f64::from(height) * 0.9).round() as u32);
            let left = rect.position.x - area.position.x;
            let top = rect.position.y - area.position.y;
            assert!(left > 0 && top > 0);
            assert!((2 * left + rect.size.width as i32 - width as i32).abs() <= 1);
            assert!((2 * top + rect.size.height as i32 - height as i32).abs() <= 1);
        }
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1040),
        };
        assert_eq!(bounds(&area, 1.0).size, PhysicalSize::new(1248, 853));
        assert_eq!(bounds(&area, 1.5).size, PhysicalSize::new(1410, 930));
        let large = PhysicalRect {
            size: PhysicalSize::new(3840, 2080),
            ..area
        };
        assert_eq!(bounds(&large, 1.5).size, PhysicalSize::new(2496, 1706));
    }
}
