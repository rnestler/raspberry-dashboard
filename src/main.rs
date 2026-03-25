use chrono::Local;
use rand::Rng;

mod config;
mod homeassistant;
mod snapcast;

slint::include_modules!();

const WIDGET_COUNT: i32 = 3;

fn main() {
    let config = config::load_config();

    let dashboard = Dashboard::new().unwrap();

    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());

    // Widget switching via TAB
    let weak = dashboard.as_weak();
    dashboard.on_next_widget(move || {
        if let Some(d) = weak.upgrade() {
            let current = d.get_current_widget();
            d.set_current_widget((current + 1) % WIDGET_COUNT);
        }
    });

    // Quit via "q"
    let weak = dashboard.as_weak();
    dashboard.on_quit(move || {
        if let Some(d) = weak.upgrade() {
            d.hide().unwrap();
        }
    });

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

    // Snapcast client in background thread (SnapcastConnection is not Send)
    let snapcast_addr: std::net::SocketAddr = std::env::var("SNAPCAST_HOST")
        .unwrap_or_else(|_| "127.0.0.1:1705".to_string())
        .parse()
        .expect("invalid SNAPCAST_HOST address");
    let ui_handle = dashboard.as_weak();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            loop {
                snapcast::run_snapcast_client(snapcast_addr, ui_handle.clone()).await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    });

    // HomeAssistant polling in a separate background thread
    if let Some(ha_config) = config.homeassistant {
        let ui_handle = dashboard.as_weak();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(homeassistant::run_homeassistant_client(
                ha_config, ui_handle,
            ));
        });
    }

    dashboard.run().unwrap();
}
