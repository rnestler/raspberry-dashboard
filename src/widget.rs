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
    ///
    /// `fallback_widget` is the index of the first enabled widget — used by
    /// the Snapcast widget when playback stops.
    fn init(&mut self, dashboard: &crate::Dashboard, fallback_widget: i32);

    /// Called each time this widget becomes the active (visible) widget.
    ///
    /// Default implementation is a no-op.
    fn on_activate(&self, _dashboard: &crate::Dashboard) {}
}

/// Build the list of enabled widgets from the application config.
///
/// Widget order: HomeAssistant (0), NowPlaying/Snapcast (1), Clock (2),
/// DailyVerse (3), Quotes (4).  Optional widgets are only included when
/// their config section is present.
pub fn create_widgets(config: Config) -> Vec<Box<dyn Widget>> {
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

    widgets
}
