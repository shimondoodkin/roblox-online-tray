//! Small shared types.

/// A user's Roblox presence, from `presence.roblox.com`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    Offline,
    Online,
    InGame { place: String },
    InStudio,
    Unknown,
}
