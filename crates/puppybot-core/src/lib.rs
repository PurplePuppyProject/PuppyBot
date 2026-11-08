//! The logic of PuppyBot: its commands, the parsers of what users send, and the texts it answers
//! with, in German and English.
//!
//! The crate knows nothing of Discord, storage or the network, so the same logic serves every
//! platform the bot runs on, and its tests need none of them. The program, `crates/puppybot`,
//! connects it to Discord, storage and the network. The dependencies of this crate are only crates
//! without I/O; `make lint` checks that.

pub mod dice;
pub mod games;
pub mod i18n;
pub mod poll;
pub mod quotes;
pub mod text;
pub mod text_command;
