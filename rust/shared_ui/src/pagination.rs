use leptos::prelude::*;

/// Computes the ordered list of page slots to render.
///
/// Returns `Some(n)` for a page number link and `None` for an ellipsis gap.
/// Always shows the first and last page; shows up to one page on either side
/// of the current page; inserts ellipsis where there is a gap larger than one.
fn page_slots(current: u32, total: u32) -> Vec<Option<u32>> {
    if total <= 7 {
        return (1..=total).map(Some).collect();
    }

    let mut set = std::collections::BTreeSet::new();
    set.insert(1);
    set.insert(total);
    for p in current.saturating_sub(1)..=current.saturating_add(1) {
        if p >= 1 && p <= total {
            set.insert(p);
        }
    }

    let sorted: Vec<u32> = set.into_iter().collect();
    let mut result: Vec<Option<u32>> = Vec::new();
    for (i, &p) in sorted.iter().enumerate() {
        if i > 0 && p > sorted[i - 1] + 1 {
            result.push(None);
        }
        result.push(Some(p));
    }
    result
}

/// Reusable SSR-friendly pagination bar.
///
/// Renders `<a>` links of the form `{base_url}?page={n}`.
/// Hidden when `total_pages <= 1`.
#[component]
pub fn Pagination(current_page: u32, total_pages: u32, base_url: String) -> impl IntoView {
    if total_pages <= 1 {
        return view! { <div></div> }.into_any();
    }

    let slots = page_slots(current_page, total_pages);
    let prev_page = current_page.saturating_sub(1);
    let next_page = (current_page + 1).min(total_pages);
    let has_prev = current_page > 1;
    let has_next = current_page < total_pages;

    let base = base_url.clone();

    view! {
        <nav class="flex items-center justify-center gap-1 mt-6">
            // Previous button
            {if has_prev {
                let href = format!("{}?page={}", base_url, prev_page);
                view! {
                    <a href=href
                        class="px-3 py-2 rounded-xl text-sm text-text-secondary border border-border hover:bg-surface-alpha hover:text-text-primary transition-colors"
                    >
                        "← Prev"
                    </a>
                }.into_any()
            } else {
                view! {
                    <span class="px-3 py-2 rounded-xl text-sm text-text-disabled border border-border opacity-40 cursor-not-allowed">
                        "← Prev"
                    </span>
                }.into_any()
            }}

            // Page number slots
            {slots.into_iter().map(|slot| {
                match slot {
                    None => view! {
                        <span class="px-2 py-2 text-sm text-text-disabled">"…"</span>
                    }.into_any(),
                    Some(n) if n == current_page => view! {
                        <span class="px-3 py-2 rounded-xl text-sm font-semibold bg-primary text-white">
                            {n.to_string()}
                        </span>
                    }.into_any(),
                    Some(n) => {
                        let href = format!("{}?page={}", base, n);
                        view! {
                            <a href=href
                                class="px-3 py-2 rounded-xl text-sm text-text-secondary border border-border hover:bg-surface-alpha hover:text-text-primary transition-colors"
                            >
                                {n.to_string()}
                            </a>
                        }.into_any()
                    }
                }
            }).collect::<Vec<_>>()}

            // Next button
            {if has_next {
                let href = format!("{}?page={}", base_url, next_page);
                view! {
                    <a href=href
                        class="px-3 py-2 rounded-xl text-sm text-text-secondary border border-border hover:bg-surface-alpha hover:text-text-primary transition-colors"
                    >
                        "Next →"
                    </a>
                }.into_any()
            } else {
                view! {
                    <span class="px-3 py-2 rounded-xl text-sm text-text-disabled border border-border opacity-40 cursor-not-allowed">
                        "Next →"
                    </span>
                }.into_any()
            }}
        </nav>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_slots_small_total() {
        let slots: Vec<_> = page_slots(3, 5).into_iter().flatten().collect();
        assert_eq!(slots, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_page_slots_at_start() {
        let slots = page_slots(1, 10);
        assert_eq!(slots[0], Some(1));
        assert_eq!(slots[1], Some(2));
        assert!(slots.contains(&None));
        assert_eq!(slots.last(), Some(&Some(10)));
    }

    #[test]
    fn test_page_slots_in_middle() {
        let slots = page_slots(5, 10);
        assert_eq!(slots[0], Some(1));
        assert!(slots.contains(&None));
        assert!(slots.contains(&Some(4)));
        assert!(slots.contains(&Some(5)));
        assert!(slots.contains(&Some(6)));
        assert_eq!(slots.last(), Some(&Some(10)));
    }

    #[test]
    fn test_page_slots_at_end() {
        let slots = page_slots(10, 10);
        assert_eq!(slots[0], Some(1));
        assert!(slots.contains(&None));
        assert_eq!(slots.last(), Some(&Some(10)));
    }

    #[test]
    fn test_page_slots_exactly_seven() {
        let slots: Vec<_> = page_slots(4, 7).into_iter().flatten().collect();
        assert_eq!(slots, vec![1, 2, 3, 4, 5, 6, 7]);
    }
}
