use chrono::Local;
use rand::Rng;

slint::include_modules!();

fn main() {
    let dashboard = Dashboard::new().unwrap();

    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());

    // Randomize position every 5 seconds
    let weak = dashboard.as_weak();
    let position_timer = slint::Timer::default();
    position_timer.start(
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

            let text_width: f32 = 400.0;
            let text_height: f32 = 80.0;

            let max_x = (window_width - text_width).max(0.0);
            let max_y = (window_height - text_height).max(0.0);

            let mut rng = rand::rng();
            let x: f32 = rng.random_range(0.0..=max_x);
            let y: f32 = rng.random_range(0.0..=max_y);

            dashboard.set_time_x(x);
            dashboard.set_time_y(y);
        },
    );

    // Update clock every second
    let weak = dashboard.as_weak();
    let clock_timer = slint::Timer::default();
    clock_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            let Some(dashboard) = weak.upgrade() else {
                return;
            };
            let now = Local::now();
            dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());
        },
    );

    dashboard.run().unwrap();
}
