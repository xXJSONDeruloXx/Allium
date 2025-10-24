mod app;
mod apps;
mod entry_list;
mod games;
mod recents;
#[allow(dead_code, unused_imports)] // TODO: Remove once SearchResultsView is integrated
mod search_results;
mod settings;
mod toast;

pub use app::App;
pub use apps::Apps;
pub use games::Games;
pub use recents::Recents;
#[allow(unused_imports)] // TODO: Remove once SearchResultsView is integrated into app
pub use search_results::SearchResultsView;
pub use settings::Settings;
pub use toast::Toast;
