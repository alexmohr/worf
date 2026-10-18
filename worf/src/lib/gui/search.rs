//! Matching entries against the search and sorting them.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use gtk4::{FlowBoxChild, Ordering, prelude::WidgetExt};
use regex::Regex;

use super::{ArcMenuMap, MenuItem};
use crate::config::{Config, MatchMethod, SortOrder};

pub(super) fn into_core_order(gtk_order: Ordering) -> core::cmp::Ordering {
    match gtk_order {
        Ordering::Smaller => core::cmp::Ordering::Less,
        Ordering::Larger => core::cmp::Ordering::Greater,
        _ => core::cmp::Ordering::Equal,
    }
}

pub(super) fn set_menu_visibility_for_search<T: Clone>(
    query: &str,
    items: &mut HashMap<FlowBoxChild, MenuItem<T>>,
    config: &Arc<RwLock<Config>>,
    search_ignored_words: Option<&Vec<Regex>>,
) {
    let (insensitive, match_method, fuzzy_min_score) = {
        let config = config.read().unwrap();
        (
            config.insensitive(),
            config.match_method(),
            config.fuzzy_min_score(),
        )
    };

    let query = if insensitive {
        query.to_lowercase()
    } else {
        query.to_owned()
    };
    let query = filtered_query(search_ignored_words, &query);

    // Fuzzy matching scores everything 0 against an empty query, so handle it here.
    if query.trim().is_empty() {
        for (fb, menu_item) in items.iter_mut() {
            menu_item.search_sort_score = menu_item.initial_sort_score;
            menu_item.visible = true;
            fb.set_visible(menu_item.visible);
        }
        return;
    }

    for (fb, menu_item) in items.iter_mut() {
        let mut menu_item_search = format!(
            "{} {}",
            menu_item.action.as_deref().unwrap_or_default(),
            menu_item.label
        );
        if insensitive {
            menu_item_search = menu_item_search.to_lowercase();
        }

        let (search_sort_score, visible) = match match_method {
            MatchMethod::Fuzzy => {
                let mut score = strsim::jaro_winkler(&query, &menu_item_search);
                if score == 0.0 {
                    score = -1.0;
                }

                (score, score > fuzzy_min_score && score > 0.0)
            }
            MatchMethod::Contains => {
                if menu_item_search.contains(&query) {
                    (1.0, true)
                } else {
                    (0.0, false)
                }
            }
            MatchMethod::MultiContains => {
                let contains = query.split(' ').all(|x| menu_item_search.contains(x));
                (if contains { 1.0 } else { 0.0 }, contains)
            }
            MatchMethod::None => {
                (1.0, true) // items are always shown
            }
        };

        menu_item.search_sort_score = search_sort_score + menu_item.initial_sort_score;
        menu_item.visible = visible;
        fb.set_visible(menu_item.visible);
    }
}

#[must_use]
pub fn filtered_query(search_ignored_words: Option<&Vec<Regex>>, query: &str) -> String {
    let mut query = query.to_owned();
    if let Some(s) = search_ignored_words.as_ref() {
        s.iter().for_each(|rgx| {
            query = rgx.replace_all(&query, "").to_string();
        });
    }
    query
}

pub(super) fn sort_flow_box_childs<T: Clone>(
    child1: &FlowBoxChild,
    child2: &FlowBoxChild,
    items_lock: &ArcMenuMap<T>,
) -> Ordering {
    // Hidden children go last, so they never get between visible ones.
    match (child1.is_visible(), child2.is_visible()) {
        (false, false) => return Ordering::Equal,
        (false, true) => return Ordering::Larger,
        (true, false) => return Ordering::Smaller,
        (true, true) => {}
    }

    let lock = items_lock.read().unwrap();
    sort_menu_items_by_score(lock.get(child1), lock.get(child2))
}

pub(super) fn sort_menu_items_by_score<T: Clone>(
    m1: Option<&MenuItem<T>>,
    m2: Option<&MenuItem<T>>,
) -> Ordering {
    match (m1, m2) {
        (Some(menu1), Some(menu2)) => {
            fn compare(a: f64, b: f64) -> Ordering {
                if a > b {
                    Ordering::Smaller
                } else if a < b {
                    Ordering::Larger
                } else {
                    Ordering::Equal
                }
            }

            if menu1.search_sort_score > 0.0 || menu2.search_sort_score > 0.0 {
                compare(menu1.search_sort_score, menu2.search_sort_score)
            } else {
                compare(menu1.initial_sort_score, menu2.initial_sort_score)
            }
        }
        (Some(_), None) => Ordering::Larger,
        (None, Some(_)) => Ordering::Smaller,
        (None, None) => Ordering::Equal,
    }
}

/// Sorts menu items in alphabetical order, while maintaining the initial score
pub fn apply_sort<T: Clone>(items: &mut [MenuItem<T>], order: &SortOrder) {
    match order {
        SortOrder::Default => {}
        SortOrder::Alphabetical => {
            // we won't deal w/ enough items that this matters
            #[allow(clippy::cast_precision_loss)]
            let item_count = items.len() as f64;
            let special_score = item_count;
            // The alphabetical position is only a tie-breaker. All positions together
            // stay below 1e-6, so they do not outweigh the search score (0..1).
            let regular_step = 1e-6 / item_count.max(1.0);
            let mut regular_score = 0.0;
            items.sort_by(|l, r| r.label.cmp(&l.label));

            for item in items.iter_mut() {
                if item.initial_sort_score == 0.0 {
                    item.initial_sort_score += regular_score;
                    regular_score += regular_step;
                } else {
                    item.initial_sort_score += special_score;
                }
            }

            items.sort_by(|l, r| into_core_order(sort_menu_items_by_score(Some(l), Some(r))));
        }
    }
}
