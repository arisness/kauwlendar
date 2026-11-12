pub mod input;
pub mod kindle;

pub use input::{InputController, UserAction};
#[allow(unused_imports)]
pub use kindle::{
    current_utc_now, enter_deep_sleep, find_fbink_bin, get_battery_percent,
    get_screen_dimensions, get_wifi_state, is_kindle, set_wifi, wait_for_network,
    DeepSleepOutcome, DeepSleepReport, KindleGuard,
};

