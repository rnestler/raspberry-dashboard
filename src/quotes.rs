use rand::RngExt;

use crate::config::{QuoteItem, QuotesConfig};
use crate::widget::Widget;

const WIDGET_ID: i32 = 4;

/// Quotes widget — displays a random user-configured quote.
///
/// No background thread.  A new random quote is selected each time the
/// widget becomes visible (`on_activate`), and once during `init` so
/// the widget has content on first display.
pub struct QuotesWidget {
    items: Vec<QuoteItem>,
}

impl QuotesWidget {
    pub fn new(config: QuotesConfig) -> Self {
        Self {
            items: config.items,
        }
    }
}

impl Widget for QuotesWidget {
    fn id(&self) -> i32 {
        WIDGET_ID
    }

    fn init(&mut self, dashboard: &crate::Dashboard) {
        set_random_quote(&self.items, dashboard);
    }

    fn on_activate(&self, dashboard: &crate::Dashboard) {
        set_random_quote(&self.items, dashboard);
    }
}

/// Pick a uniformly random quote from `items` and push it to the dashboard.
/// Does nothing if `items` is empty.
fn set_random_quote(items: &[QuoteItem], dashboard: &crate::Dashboard) {
    if items.is_empty() {
        return;
    }
    let idx = rand::rng().random_range(0..items.len());
    let item = &items[idx];
    dashboard.set_quote_text(item.text.clone().into());
    dashboard.set_quote_source(item.source.clone().unwrap_or_default().into());
}
