//! A command-line value that may be left blank on purpose.
//!
//! A CI template that has nothing to say about a setting usually still names the
//! variable and sets it to an empty string, as a workflow does for an unset input.
//! clap reads that as a value and rejects it, so the run fails for a variable nobody
//! meant to set.

use std::ffi::OsStr;

use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command, Error};

/// What a blank-able argument holds: the value, or nothing because it was blank.
#[derive(Clone, Copy)]
pub struct Setting<T>(Option<T>);

impl<T: Copy> Setting<T> {
    /// The value an optional argument carried, when it carried one.
    pub fn of(given: Option<Self>) -> Option<T> {
        given.and_then(|setting| setting.0)
    }
}

/// Wraps another parser so an empty string means "not set" instead of an error.
#[derive(Clone)]
pub struct OrUnset<P>(pub P);

impl<P> TypedValueParser for OrUnset<P>
where
    P: TypedValueParser,
    P::Value: Clone + Send + Sync + 'static,
{
    type Value = Setting<P::Value>;

    fn parse_ref(
        &self,
        command: &Command,
        argument: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        if value.is_empty() {
            return Ok(Setting(None));
        }
        self.0
            .parse_ref(command, argument, value)
            .map(|parsed| Setting(Some(parsed)))
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        self.0.possible_values()
    }
}
