use std::collections::HashMap;

use rqx_core::cost::{SizeSrc, SizeVar};

use super::types::TypeFact;

/// How large a value is, as far as the source says: a fixed amount, or as large as one
/// of the function's size variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeRef {
    Const,
    Var(u16),
}

/// What one name in a function stands for.
#[derive(Clone, Debug, Default)]
pub struct Binding {
    pub fact: Option<TypeFact>,
    /// The size of what the name holds, or its value where it is a count. Set when the
    /// name was introduced from something whose size is known, so a name that copies
    /// or measures another carries that size along.
    pub size: Option<SizeRef>,
    /// The position among the function's parameters, where it is one.
    pub parameter: Option<u8>,
}

/// The names of one function body and the size variables they give rise to. Built as the
/// body is read, so a name only knows what was written above it.
#[derive(Default)]
pub struct Env {
    bindings: HashMap<String, Binding>,
    vars: Vec<SizeVar>,
    /// The members of the class the function belongs to, by name.
    fields: HashMap<String, TypeFact>,
    /// What the function calls the object it belongs to, in a language that names it
    /// as an ordinary parameter, as Go does.
    receiver: Option<String>,
}

impl Env {
    pub fn with_fields(fields: HashMap<String, TypeFact>) -> Self {
        Self {
            fields,
            ..Self::default()
        }
    }

    pub fn set_receiver(&mut self, name: &str) {
        self.receiver = Some(name.to_owned());
    }

    pub fn receiver(&self) -> Option<&str> {
        self.receiver.as_deref()
    }

    pub fn bind(&mut self, name: &str, binding: Binding) {
        self.bindings.insert(name.to_owned(), binding);
    }

    pub fn binding(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }

    pub fn field(&self, name: &str) -> Option<&TypeFact> {
        self.fields.get(name)
    }

    /// The size variable for `name` from `src`, made on first use. One variable per
    /// name and origin, so every loop over `items` runs over the same size.
    pub fn var(&mut self, name: &str, src: SizeSrc) -> u16 {
        if let Some(found) = self
            .vars
            .iter()
            .position(|var| var.name == name && var.src == src)
        {
            return found as u16;
        }
        self.vars.push(SizeVar {
            name: name.to_owned(),
            src,
        });
        (self.vars.len() - 1) as u16
    }

    pub fn vars(&self) -> &[SizeVar] {
        &self.vars
    }

    pub fn into_vars(self) -> Vec<SizeVar> {
        self.vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_name_from_one_place_is_one_size() {
        let mut env = Env::default();
        let items = env.var("items", SizeSrc::Param(0));
        assert_eq!(env.var("items", SizeSrc::Param(0)), items);
        assert_ne!(env.var("items", SizeSrc::Local), items);
        assert_ne!(env.var("rows", SizeSrc::Param(1)), items);
        assert_eq!(env.vars().len(), 3);
    }
}
