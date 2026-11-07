//! Stopping on a signal: SIGTERM, which Docker and service managers send, or SIGINT, from Ctrl+C.

use std::io;

use tokio::signal::unix::{Signal, SignalKind, signal};

/// The signal that stopped PuppyBot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// SIGTERM.
    Terminate,
    /// SIGINT.
    Interrupt,
}

impl Stop {
    /// The name of the signal, e.g. `SIGTERM`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Terminate => "SIGTERM",
            Self::Interrupt => "SIGINT",
        }
    }
}

/// The signals that stop PuppyBot, caught from when this is made: so make it before saying that
/// PuppyBot started, or a signal could end it without stopping it properly.
#[derive(Debug)]
pub struct Signals {
    terminate: Signal,
    interrupt: Signal,
}

impl Signals {
    /// Catches SIGTERM and SIGINT, within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// The error of catching the signals, which happens only without a runtime that can.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminate: signal(SignalKind::terminate())?,
            interrupt: signal(SignalKind::interrupt())?,
        })
    }

    /// Waits for the first of the signals.
    pub async fn recv(&mut self) -> Stop {
        tokio::select! {
            _ = self.terminate.recv() => Stop::Terminate,
            _ = self.interrupt.recv() => Stop::Interrupt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Stop;

    #[test]
    fn names_of_the_signals() {
        assert_eq!(Stop::Terminate.name(), "SIGTERM");
        assert_eq!(Stop::Interrupt.name(), "SIGINT");
    }
}
