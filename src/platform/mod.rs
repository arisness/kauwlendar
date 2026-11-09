pub mod input;
pub mod kindle;

pub use input::InputController;
pub use kindle::{
    find_fbink_bin, get_battery_percent, get_screen_dimensions, is_kindle,
    wait_for_network, KindleGuard,
};

