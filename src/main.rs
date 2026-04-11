use chrono::Local;
use std::rc::Rc;

mod clock;
mod config;
mod dailyverse;
mod homeassistant;
mod quotes;
mod snapcast;
mod weather;
mod widget;

slint::include_modules!();

fn main() {
    env_logger::init();
    let config = config::load_config();
    let widget_cycle_secs = config.widget_cycle_secs;

    // Build the widget controller from config.
    let mut controller = widget::create_widgets(config);

    let dashboard = Dashboard::new().unwrap();

    // Set initial time.
    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());

    // Set initial widget and initialise every widget (main-thread setup +
    // background thread spawning).
    controller.init_all(&dashboard);

    // Wrap in Rc for sharing with closures.
    let controller = Rc::new(controller);

    // Auto-cycle timer — created unconditionally so we can share it with the
    // TAB callback (to restart it on manual switch).
    let cycle_timer = Rc::new(slint::Timer::default());

    // Widget switching via TAB — cycles through ALL enabled widgets
    // (including inactive ones).  Also restarts the auto-cycle timer.
    let weak = dashboard.as_weak();
    let cycle_timer_tab = Rc::clone(&cycle_timer);
    let ctrl = Rc::clone(&controller);
    dashboard.on_next_widget(move || {
        if let Some(d) = weak.upgrade() {
            ctrl.advance(&d, false);
            // Restart the cycle timer so the user gets a full interval after
            // a manual switch.
            if cycle_timer_tab.running() {
                cycle_timer_tab.restart();
            }
        }
    });

    // A widget has been deactivated — if the currently displayed widget is
    // inactive, switch to the next active one.
    let weak = dashboard.as_weak();
    let ctrl = Rc::clone(&controller);
    dashboard.on_deactivate_widget(move || {
        if let Some(d) = weak.upgrade() {
            ctrl.deactivate_current(&d);
        }
    });

    // A background thread wants to switch to a specific widget by ID.
    let weak = dashboard.as_weak();
    let ctrl = Rc::clone(&controller);
    dashboard.on_activate_widget(move |id| {
        if let Some(d) = weak.upgrade() {
            ctrl.switch_to(&d, id);
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

    // Auto-cycle: advance to next *active* widget every N seconds.
    if let Some(secs) = widget_cycle_secs
        && controller.len() > 1
    {
        let weak = dashboard.as_weak();
        let ctrl = Rc::clone(&controller);
        cycle_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(secs),
            move || {
                let Some(d) = weak.upgrade() else {
                    return;
                };
                ctrl.advance(&d, true);
            },
        );
    }

    dashboard.run().unwrap();
}
