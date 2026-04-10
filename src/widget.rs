use crate::config::Config;

/// Common interface for all dashboard widgets.
///
/// Each widget lives in its own module and implements this trait.
/// The [`create_widgets`] factory inspects the loaded [`Config`] and
/// returns only the widgets whose configuration section is present
/// (the Clock widget is always included).
pub trait Widget {
    /// Slint `current-widget` index for this widget.
    fn index(&self) -> i32;

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
/// callbacks (`next-widget`, `deactivate-widget`) and the auto-cycle
/// timer all delegate to methods on this struct.
pub struct WidgetController {
    widgets: Vec<Box<dyn Widget>>,
}

impl WidgetController {
    /// Initialise every widget (main-thread setup + background thread spawning).
    pub fn init_all(&mut self, dashboard: &crate::Dashboard) {
        for w in self.widgets.iter_mut() {
            w.init(dashboard);
        }
    }

    /// The index of the first enabled widget (used as initial display).
    pub fn first_index(&self) -> i32 {
        self.widgets[0].index()
    }

    /// Number of enabled widgets.
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    /// Advance to the next widget, wrapping around.
    ///
    /// When `active_only` is true, inactive widgets are skipped (used by
    /// the auto-cycle timer and `deactivate_current`).  When false, all
    /// enabled widgets are candidates (used by manual TAB switching).
    ///
    /// Calls `on_activate` on the newly visible widget.
    pub fn advance(&self, dashboard: &crate::Dashboard, active_only: bool) {
        let current = dashboard.get_current_widget();
        let len = self.widgets.len();
        let cur_pos = self
            .widgets
            .iter()
            .position(|w| w.index() == current)
            .unwrap_or(0);

        for offset in 1..=len {
            let pos = (cur_pos + offset) % len;
            let w = &self.widgets[pos];
            if !active_only || w.is_active() {
                dashboard.set_current_widget(w.index());
                w.on_activate(dashboard);
                return;
            }
        }
        // All widgets inactive (unlikely) — stay on current.
    }

    /// A widget has signalled that it became inactive.
    ///
    /// If the currently displayed widget is inactive, advance to the
    /// next active widget.
    pub fn deactivate_current(&self, dashboard: &crate::Dashboard) {
        let current = dashboard.get_current_widget();
        let is_inactive = self
            .widgets
            .iter()
            .find(|w| w.index() == current)
            .is_some_and(|w| !w.is_active());
        if is_inactive {
            self.advance(dashboard, true);
        }
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

    if let Some(ha_config) = config.homeassistant {
        widgets.push(Box::new(crate::homeassistant::HomeAssistantWidget::new(
            ha_config,
        )));
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

    WidgetController { widgets }
}
