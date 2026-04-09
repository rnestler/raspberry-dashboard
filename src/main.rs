use chrono::Local;
use std::rc::Rc;

mod clock;
mod config;
mod dailyverse;
mod homeassistant;
mod quotes;
mod snapcast;
mod widget;

slint::include_modules!();

fn main() {
    env_logger::init();
    let config = config::load_config();
    let widget_cycle_secs = config.widget_cycle_secs;

    // Build the list of enabled widgets from config.
    let mut widgets = widget::create_widgets(config);
    let enabled_indices: Vec<i32> = widgets.iter().map(|w| w.index()).collect();
    let fallback_widget = enabled_indices[0];

    let dashboard = Dashboard::new().unwrap();

    // Set initial time and active widget.
    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());
    dashboard.set_current_widget(fallback_widget);

    // Initialise every widget (main-thread setup + background thread spawning).
    for w in widgets.iter_mut() {
        w.init(&dashboard, fallback_widget);
    }

    // Wrap in Rc for sharing with closures (advance_widget, TAB, cycle timer).
    let widgets: Rc<Vec<Box<dyn widget::Widget>>> = Rc::new(widgets);

    // Helper: advance to the next enabled widget, wrapping around.
    // Calls on_activate for the newly active widget.
    let advance_widget = {
        let enabled_indices = enabled_indices.clone();
        let widgets = Rc::clone(&widgets);
        move |dashboard: &Dashboard| {
            let current = dashboard.get_current_widget();
            let next_pos = enabled_indices
                .iter()
                .position(|&w| w == current)
                .map(|pos| (pos + 1) % enabled_indices.len())
                .unwrap_or(0);
            let next_widget = enabled_indices[next_pos];
            dashboard.set_current_widget(next_widget);
            if let Some(w) = widgets.iter().find(|w| w.index() == next_widget) {
                w.on_activate(dashboard);
            }
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

    // Update clock every second (dashboard-level concern — all widgets show
    // the time via the shared `current-time` property).
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
    if let Some(secs) = widget_cycle_secs
        && enabled_indices.len() > 1
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

    dashboard.run().unwrap();
}
