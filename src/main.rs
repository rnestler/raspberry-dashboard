use chrono::Local;
use rand::RngExt;
use std::rc::Rc;

mod config;
mod dailyverse;
mod homeassistant;
mod snapcast;

slint::include_modules!();

fn main() {
    env_logger::init();
    let config = config::load_config();

    // Build the list of enabled widget indices in display order.
    // Widgets 1 (NowPlaying) and 2 (Clock) are always available.
    // Widgets 0 (HomeAssistant) and 3 (DailyVerse) require config.
    let mut enabled_widgets: Vec<i32> = Vec::new();
    if config.homeassistant.is_some() {
        enabled_widgets.push(0);
    }
    enabled_widgets.push(1); // NowPlaying (Snapcast always starts)
    enabled_widgets.push(2); // Clock
    if config.daily_verse.is_some() {
        enabled_widgets.push(3);
    }

    let dashboard = Dashboard::new().unwrap();

    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());
    dashboard.set_current_widget(enabled_widgets[0]);

    // Helper: advance to the next enabled widget, wrapping around.
    let advance_widget = {
        let enabled_widgets = enabled_widgets.clone();
        move |dashboard: &Dashboard| {
            let current = dashboard.get_current_widget();
            let next_pos = enabled_widgets
                .iter()
                .position(|&w| w == current)
                .map(|pos| (pos + 1) % enabled_widgets.len())
                .unwrap_or(0);
            dashboard.set_current_widget(enabled_widgets[next_pos]);
        }
    };

    // Auto-cycle timer — created unconditionally so we can share it with the
    // TAB callback (to restart it on manual switch).
    let cycle_timer = Rc::new(slint::Timer::default());

    // Widget switching via TAB — also restarts the auto-cycle timer.
    let weak = dashboard.as_weak();
    let cycle_timer_tab = Rc::clone(&cycle_timer);
    let advance_widget_tab = advance_widget.clone();
    dashboard.on_next_widget(move || {
        if let Some(d) = weak.upgrade() {
            advance_widget_tab(&d);
            // Restart the cycle timer so the user gets a full interval after
            // a manual switch.
            if cycle_timer_tab.running() {
                cycle_timer_tab.restart();
            }
        }
    });

    // Quit via "q"
    let weak = dashboard.as_weak();
    dashboard.on_quit(move || {
        if let Some(d) = weak.upgrade() {
            d.hide().unwrap();
        }
    });

    // Randomize clock position every 5 seconds
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

    // Auto-cycle: advance widget every N seconds if configured.
    if let Some(secs) = config.widget_cycle_secs
        && enabled_widgets.len() > 1
    {
        let weak = dashboard.as_weak();
        cycle_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(secs),
            move || {
                let Some(d) = weak.upgrade() else {
                    return;
                };
                advance_widget(&d);
            },
        );
    }

    // Snapcast client in background thread (SnapcastConnection is not Send)
    let snapcast_addr: std::net::SocketAddr = std::env::var("SNAPCAST_HOST")
        .unwrap_or_else(|_| "127.0.0.1:1705".to_string())
        .parse()
        .expect("invalid SNAPCAST_HOST address");
    let ui_handle = dashboard.as_weak();
    let fallback_widget = enabled_widgets[0];
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            loop {
                snapcast::run_snapcast_client(snapcast_addr, ui_handle.clone(), fallback_widget)
                    .await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    });

    // Daily verse polling in a separate background thread
    if let Some(dv_config) = config.daily_verse {
        let ui_handle = dashboard.as_weak();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(dailyverse::run_daily_verse_client(dv_config, ui_handle));
        });
    }

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
