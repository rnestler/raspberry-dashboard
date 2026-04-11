use std::cell::Cell;

use crate::config::Config;

/// Common interface for all dashboard widgets.
///
/// Each widget lives in its own module and implements this trait.
/// The [`create_widgets`] factory inspects the loaded [`Config`] and
/// returns only the widgets whose configuration section is present
/// (the Clock widget is always included).
pub trait Widget {
    /// Slint `current-widget` ID for this widget.
    fn id(&self) -> i32;

    /// Called once at startup on the **main thread**.
    ///
    /// Use this for initial UI property setup, creating main-thread timers,
    /// and/or spawning background threads.
    fn init(&mut self, dashboard: &crate::Dashboard);

    /// Called each time this widget becomes the visible widget.
    ///
    /// Default implementation is a no-op.
    fn on_activate(&self, _dashboard: &crate::Dashboard) {}

    /// Whether this widget is currently active.
    ///
    /// Inactive widgets are skipped by the auto-cycle timer but can still
    /// be reached via manual TAB switching.  Default: always active.
    fn is_active(&self) -> bool {
        true
    }
}

/// Owns the widget list and centralises all widget-switching logic.
///
/// Lives on the main thread as `Rc<WidgetController>`.  The Slint
/// callbacks (`next-widget`, `activate-widget`, `deactivate-widget`) and
/// the auto-cycle timer all delegate to methods on this struct.
///
/// The controller is the source of truth for which widget position is
/// currently displayed.  It tracks position internally via a `Cell<usize>`
/// so callers never need to look up the current widget from Slint.
pub struct WidgetController {
    widgets: Vec<Box<dyn Widget>>,
    current: Cell<usize>,
}

impl WidgetController {
    /// Set the initial widget and initialise every widget
    /// (main-thread setup + background thread spawning).
    pub fn init_all(&mut self, dashboard: &crate::Dashboard) {
        self.current.set(0);
        dashboard.set_current_widget(self.widgets[0].id());
        for w in self.widgets.iter_mut() {
            w.init(dashboard);
        }
    }

    /// Number of enabled widgets.
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    /// Switch directly to the widget with the given `id`.
    ///
    /// Used by the `activate-widget` Slint callback so that background
    /// threads (e.g. Snapcast) can request a specific widget.  Updates
    /// internal position, sets the Slint property, and calls `on_activate`.
    pub fn switch_to(&self, dashboard: &crate::Dashboard, id: i32) {
        if let Some(pos) = self.widgets.iter().position(|w| w.id() == id) {
            self.show(dashboard, pos);
        }
    }

    /// Advance to the next widget, wrapping around.
    ///
    /// When `active_only` is true, inactive widgets are skipped (used by
    /// the auto-cycle timer and `deactivate_current`).  When false, all
    /// enabled widgets are candidates (used by manual TAB switching).
    pub fn advance(&self, dashboard: &crate::Dashboard, active_only: bool) {
        let cur = self.current.get();
        let len = self.widgets.len();
        for offset in 1..=len {
            let pos = (cur + offset) % len;
            if !active_only || self.widgets[pos].is_active() {
                self.show(dashboard, pos);
                return;
            }
        }
        // All widgets inactive (unlikely) — stay on current.
    }

    /// If the currently displayed widget is inactive, advance to the
    /// next active widget.
    pub fn deactivate_current(&self, dashboard: &crate::Dashboard) {
        if !self.widgets[self.current.get()].is_active() {
            self.advance(dashboard, true);
        }
    }

    /// Internal helper: switch to the widget at `pos`.
    fn show(&self, dashboard: &crate::Dashboard, pos: usize) {
        self.current.set(pos);
        let w = &self.widgets[pos];
        dashboard.set_current_widget(w.id());
        w.on_activate(dashboard);
    }
}

/// Build the list of enabled widgets from the application config and
/// return a [`WidgetController`] that owns them.
///
/// Widget order: HomeAssistant (0), NowPlaying/Snapcast (1), Clock (2),
/// DailyVerse (3), Quotes (4).  Optional widgets are only included when
/// their config section is present.
pub fn create_widgets(config: Config) -> WidgetController {
    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();
    let ha_token = crate::config::homeassistant_token();

    if let Some(ha_config) = config.homeassistant {
        if let Some(token) = ha_token.clone() {
            widgets.push(Box::new(crate::homeassistant::HomeAssistantWidget::new(
                ha_config, token,
            )));
        } else {
            log::warn!(
                "HomeAssistant config present but HOMEASSISTANT_TOKEN not set – skipping widget"
            );
        }
    }
    if let Some(sc_config) = config.snapcast {
        widgets.push(Box::new(crate::snapcast::SnapcastWidget::new(sc_config)));
    }
    widgets.push(Box::new(crate::clock::ClockWidget::new()));
    if let Some(dv_config) = config.daily_verse {
        widgets.push(Box::new(crate::dailyverse::DailyVerseWidget::new(
            dv_config,
        )));
    }
    if let Some(quotes_config) = config.quotes {
        widgets.push(Box::new(crate::quotes::QuotesWidget::new(quotes_config)));
    }

    WidgetController {
        widgets,
        current: Cell::new(0),
    }
}
