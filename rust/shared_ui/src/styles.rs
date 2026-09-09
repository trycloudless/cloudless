pub const INPUT_CLASS: &str = "w-full bg-input border border-border rounded-[var(--radius-base)] px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-ring transition-all";

pub const INPUT_COMPACT_CLASS: &str = "w-full bg-input border border-border rounded-[var(--radius-base)] px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-ring transition-all";

pub const LABEL_CLASS: &str = "block text-sm font-medium text-text-secondary mb-2";

pub const LABEL_COMPACT_CLASS: &str = "block text-sm font-medium text-text-secondary mb-1.5";

pub const BTN_PRIMARY_CLASS: &str = "w-full btn-gradient text-white py-3 rounded-[var(--radius-base)] font-semibold text-lg shadow-lg shadow-primary-glow hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none";

pub const BTN_SECONDARY_CLASS: &str = "border border-border text-text-secondary px-8 py-3 rounded-[var(--radius-base)] font-semibold hover:bg-surface transition-all";

// ── Elevation Card Classes ─────────────────────────────────────────
// Level 1: Primary controls (backup jobs, security summary, auto-backup)
pub const CARD_LEVEL_1: &str =
    "glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl";
// Level 2: Infrastructure (devices, storage, stats)
pub const CARD_LEVEL_2: &str =
    "glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl";
// Level 3: Informational (appearance, metadata, device info)
pub const CARD_LEVEL_3: &str =
    "glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl";
