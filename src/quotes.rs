use rand::RngExt;

use crate::config::QuoteItem;

/// Widget index for the Quotes widget in the dashboard.
pub const WIDGET_INDEX: i32 = 4;

/// Pick a uniformly random quote from `items` and push it to the dashboard.
/// Does nothing if `items` is empty.
pub fn set_random_quote(items: &[QuoteItem], dashboard: &crate::Dashboard) {
    if items.is_empty() {
        return;
    }
    let idx = rand::rng().random_range(0..items.len());
    let item = &items[idx];
    dashboard.set_quote_text(item.text.clone().into());
    dashboard.set_quote_source(item.source.clone().unwrap_or_default().into());
}
