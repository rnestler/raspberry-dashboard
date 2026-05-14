use std::rc::Rc;

mod clock;
mod config;
mod dailyverse;
mod homeassistant;
mod quotes;
mod remote;
mod snapcast;
mod weather;
mod widget;

slint::include_modules!();

fn main() {
    env_logger::init();
    let mut config = config::load_config();
    let widget_cycle_secs = config.widget_cycle_secs;
    let remote_control_config = config.remote_control.take();

    let dashboard = Dashboard::new().unwrap();

    // Build the widget controller from config.
    let mut controller = widget::create_widgets(config, dashboard.as_weak());

    // Set initial time.
    controller.update_time();

    // Set initial widget and initialise every widget (main-thread setup +
    // background thread spawning).
    controller.init_all();

    // Spawn the remote-control HTTP server if configured.
    if let Some(rc_config) = remote_control_config {
        let map = controller.widget_name_map();
        remote::spawn(rc_config, map, dashboard.as_weak());
    }

    // Wrap in Rc for sharing with closures.
    let controller = Rc::new(controller);

    // Auto-cycle timer — created unconditionally so we can share it with the
    // TAB callback (to restart it on manual switch).
    let cycle_timer = Rc::new(slint::Timer::default());

    // Widget switching via TAB — cycles through ALL enabled widgets
    // (including inactive ones).  Also restarts the auto-cycle timer.
    let cycle_timer_tab = Rc::clone(&cycle_timer);
    let ctrl = Rc::clone(&controller);
    dashboard.on_next_widget(move || {
        ctrl.advance(false);
        // Restart the cycle timer so the user gets a full interval after
        // a manual switch.
        if cycle_timer_tab.running() {
            cycle_timer_tab.restart();
        }
    });

    // A widget has been deactivated — if the currently displayed widget is
    // inactive, switch to the next active one.
    let ctrl = Rc::clone(&controller);
    dashboard.on_deactivate_widget(move || {
        ctrl.deactivate_current();
    });

    // A background thread wants to switch to a specific widget by ID.
    let ctrl = Rc::clone(&controller);
    dashboard.on_activate_widget(move |id| {
        ctrl.switch_to(id);
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
    let ctrl = Rc::clone(&controller);
    let clock_timer = slint::Timer::default();
    clock_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            ctrl.update_time();
        },
    );

    // Auto-cycle: advance to next *active* widget every N seconds.
    if let Some(secs) = widget_cycle_secs
        && controller.len() > 1
    {
        let ctrl = Rc::clone(&controller);
        cycle_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(secs),
            move || {
                ctrl.advance(true);
            },
        );
    }

    dashboard.run().unwrap();
}
