use crate::state::*;
use leptos::prelude::*;

use super::backup_view::BackupView;
use super::dashboard::Dashboard;
use super::files_view::FilesView;
use super::settings_view::SettingsView;
use super::sidebar::Sidebar;

#[component]
pub fn MainLayout(active_view: MainView) -> impl IntoView {
    view! {
        <div class="flex h-screen w-full">
            <Sidebar active_view=active_view />
            <main class="flex-1 overflow-y-auto p-4 md:p-8 pb-24 md:pb-8">
                <div class="max-w-5xl mx-auto">
                    {match active_view {
                        MainView::Dashboard => view! { <Dashboard /> }.into_any(),
                        MainView::Files => view! { <FilesView /> }.into_any(),
                        MainView::Backup => view! { <BackupView initial_tab="backups" /> }.into_any(),
                        MainView::BackupReports => view! { <BackupView initial_tab="reports" /> }.into_any(),
                        MainView::Settings => view! { <SettingsView /> }.into_any(),
                    }}
                </div>
            </main>
        </div>
    }
}
