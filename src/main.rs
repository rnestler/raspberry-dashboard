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

/// Advance to the next enabled widget, wrapping around.
///
/// When `active_only` is true, inactive widgets are skipped (used by
/// the auto-cycle timer).  When false, all enabled widgets are
/// candidates (used by manual TAB switching).
///
/// Calls `on_activate` on the newly visible widget.
fn advance_widget(dashboard: &Dashboard, widgets: &[Box<dyn widget::Widget>], active_only: bool) {
    let current = dashboard.get_current_widget();
    let len = widgets.len();
    let cur_pos = widgets
        .iter()
        .position(|w| w.index() == current)
        .unwrap_or(0);

    // Walk forward from the current position, looking for the next candidate.
    for offset in 1..=len {
        let pos = (cur_pos + offset) % len;
        let w = &widgets[pos];
        if !active_only || w.is_active() {
            dashboard.set_current_widget(w.index());
            w.on_activate(dashboard);
            return;
        }
    }
    // All widgets inactive (unlikely) — stay on current.
}

fn main() {
    env_logger::init();
    let config = config::load_config();
    let widget_cycle_secs = config.widget_cycle_secs;

    // Build the list of enabled widgets from config.
    let mut widgets = widget::create_widgets(config);

    let dashboard = Dashboard::new().unwrap();

    // Set initial time and active widget.
    let now = Local::now();
    dashboard.set_current_time(now.format("%H:%M:%S").to_string().into());
    dashboard.set_current_widget(widgets[0].index());

    // Initialise every widget (main-thread setup + background thread spawning).
    for w in widgets.iter_mut() {
        w.init(&dashboard);
    }

    // Wrap in Rc for sharing with closures.
    let widgets: Rc<Vec<Box<dyn widget::Widget>>> = Rc::new(widgets);

    // Auto-cycle timer — created unconditionally so we can share it with the
    // TAB callback (to restart it on manual switch).
    let cycle_timer = Rc::new(slint::Timer::default());

    // Widget switching via TAB — cycles through ALL enabled widgets
    // (including inactive ones).  Also restarts the auto-cycle timer.
    let weak = dashboard.as_weak();
    let cycle_timer_tab = Rc::clone(&cycle_timer);
    let widgets_tab = Rc::clone(&widgets);
    dashboard.on_next_widget(move || {
        if let Some(d) = weak.upgrade() {
            advance_widget(&d, &widgets_tab, false);
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
    let widgets_deact = Rc::clone(&widgets);
    dashboard.on_deactivate_widget(move || {
        if let Some(d) = weak.upgrade() {
            let current = d.get_current_widget();
            let is_current_inactive = widgets_deact
                .iter()
                .find(|w| w.index() == current)
                .is_some_and(|w| !w.is_active());
            if is_current_inactive {
                advance_widget(&d, &widgets_deact, true);
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

    // Auto-cycle: advance to next *active* widget every N seconds.
    if let Some(secs) = widget_cycle_secs
        && widgets.len() > 1
    {
        let weak = dashboard.as_weak();
        let widgets_cycle = Rc::clone(&widgets);
        cycle_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(secs),
            move || {
                let Some(d) = weak.upgrade() else {
                    return;
                };
                advance_widget(&d, &widgets_cycle, true);
            },
        );
    }

    dashboard.run().unwrap();
}
