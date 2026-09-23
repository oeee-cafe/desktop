//! What the app says to the page about the store it sells through, the same
//! whichever store that is (app_store.jinja in oeee-cafe/web).
//!
//! A build sells through one store at most: Steam (steam.rs) when Steam
//! started it, or the Microsoft Store (microsoft.rs) when it is the Store's
//! package. The page asks the same two things of either -- what the products
//! cost, and to sell one -- and hears back the same two calls, which are here
//! so neither store words them its own way.

use std::collections::BTreeMap;

/// A value as JavaScript reads it: JSON, so a string arrives quoted and
/// escaped, and cannot close its own quotes to run as script.
pub fn quoted(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("a value serialises")
}

/// Answers the page's `prices` with what each product costs, formatted by
/// the store in the player's currency. A product the store gave no price
/// for is left out, and the page shows its button without one. Guarded, as
/// every call into the page is: a page that has gone since it asked has no
/// `oeeeApp.store` to answer.
pub fn prices_script(prices: &BTreeMap<String, String>) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.prices && window.oeeeApp.store.prices({});",
        quoted(prices)
    )
}

/// Hands the page proof of what was bought: a Steam ticket, or a Microsoft
/// Store ID key. The page posts each to the site, which asks the store
/// itself; it signs nobody in, and reloads only when the site took
/// something, so it can come mid-drawing with the same care as any
/// purchase.
pub fn purchased_script(proofs: &[&str]) -> String {
    format!(
        "window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.purchased({});",
        quoted(&proofs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_are_a_map_of_product_to_price_quoted_as_javascript() {
        let prices = BTreeMap::from([
            ("3456780".to_string(), "₩5,500".to_string()),
            ("9NBLGGH4R315".to_string(), "\"$4.99\"".to_string()),
        ]);
        assert_eq!(
            prices_script(&prices),
            r#"window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.prices && window.oeeeApp.store.prices({"3456780":"₩5,500","9NBLGGH4R315":"\"$4.99\""});"#
        );
    }

    #[test]
    fn proof_of_a_purchase_is_a_list_of_strings() {
        assert_eq!(
            purchased_script(&["1400abff"]),
            r#"window.oeeeApp && window.oeeeApp.store && window.oeeeApp.store.purchased(["1400abff"]);"#
        );
    }
}
