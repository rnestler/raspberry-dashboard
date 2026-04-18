use rand::RngExt;
use slint::ComponentHandle;

use crate::widget::Widget;

const WIDGET_ID: i32 = 2;

/// The always-enabled clock widget.
///
/// Owns a 5-second timer that randomly repositions the time display
/// (screensaver effect).  The 1-second `current_time` update is a
/// dashboard-level concern and lives in `main.rs`.
pub struct ClockWidget {
    /// Kept alive so the timer is not dropped (which would stop it).
    _position_timer: Option<slint::Timer>,
}

impl ClockWidget {
    pub fn new() -> Self {
        Self {
            _position_timer: None,
        }
    }
}

impl Widget for ClockWidget {
    fn id(&self) -> i32 {
        WIDGET_ID
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        let weak = dashboard.as_weak();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(5),
            move || {
                let Some(dashboard) = weak.upgrade() else {
                    return;
                };
                let size = dashboard.window().size();
                let scale = dashboard.window().scale_factor();
                let window_width = size.width as f32 / scale;
                let window_height = size.height as f32 / scale;

                let text_width: f32 = 450.0;
                let text_height: f32 = 140.0;

                let max_x = (window_width - text_width).max(0.0);
                let max_y = (window_height - text_height).max(0.0);

                let mut rng = rand::rng();
                let x: f32 = rng.random_range(0.0..=max_x);
                let y: f32 = rng.random_range(0.0..=max_y);

                dashboard.set_time_x(x);
                dashboard.set_time_y(y);
            },
        );
        self._position_timer = Some(timer);
    }
}
