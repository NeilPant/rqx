//! The asymptotic algebra the cost analysis composes with. An [`Order`] is a sum of
//! monomials over size variables, kept as an antichain under dominance so that two
//! orders that describe the same growth are equal as values.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Index of a size variable in the function the order belongs to.
pub type SizeId = u16;

/// One variable's share of a monomial: the variable, its power and the power of its
/// logarithm.
pub type Factor = (SizeId, u8, u8);

/// A product of variable powers such as `n² log n` or `n·m`. Constant factors carry no
/// growth and are never stored, so the constant term has no representation here:
/// [`Order::one`] is the order with no terms.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Term {
    factors: Vec<Factor>,
}

impl Term {
    fn of(factors: impl IntoIterator<Item = Factor>) -> Self {
        let mut merged: Vec<Factor> = Vec::new();
        let mut factors: Vec<Factor> = factors.into_iter().collect();
        factors.sort_unstable();
        for (variable, power, logarithm) in factors {
            match merged.last_mut() {
                Some(last) if last.0 == variable => {
                    last.1 = last.1.saturating_add(power);
                    last.2 = last.2.saturating_add(logarithm);
                }
                _ => merged.push((variable, power, logarithm)),
            }
        }
        merged.retain(|&(_, power, logarithm)| power > 0 || logarithm > 0);
        Self { factors: merged }
    }

    pub fn factors(&self) -> &[Factor] {
        &self.factors
    }

    fn times(&self, other: &Self) -> Self {
        Self::of(self.factors.iter().chain(&other.factors).copied())
    }

    /// Grows at least as fast as `other` when every size goes to infinity: for each
    /// variable, a larger power, or an equal power with at least as many logarithms.
    /// `n·m` and `n²` are incomparable because each beats the other in one variable.
    fn dominates(&self, other: &Self) -> bool {
        other.factors.iter().all(|&(variable, power, logarithm)| {
            let (own_power, own_logarithm) = self.share_of(variable);
            (own_power, own_logarithm) >= (power, logarithm)
        })
    }

    fn share_of(&self, variable: SizeId) -> (u8, u8) {
        self.factors
            .iter()
            .find(|factor| factor.0 == variable)
            .map_or((0, 0), |&(_, power, logarithm)| (power, logarithm))
    }

    fn rank(&self) -> u32 {
        if self.factors.is_empty() {
            return 0;
        }
        let power: u32 = self.factors.iter().map(|f| u32::from(f.1)).sum();
        let logarithm: u32 = self.factors.iter().map(|f| u32::from(f.2)).sum();
        1 + 4 * power + logarithm.min(3)
    }
}

/// How a function's cost grows. `exp` stands for anything exponential and absorbs
/// every other term.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Order {
    #[serde(default, skip_serializing_if = "is_false")]
    exp: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    terms: Vec<Term>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Order {
    pub fn one() -> Self {
        Self::default()
    }

    pub fn exponential() -> Self {
        Self {
            exp: true,
            terms: Vec::new(),
        }
    }

    pub fn var(variable: SizeId) -> Self {
        Self::from_factor((variable, 1, 0))
    }

    pub fn log_var(variable: SizeId) -> Self {
        Self::from_factor((variable, 0, 1))
    }

    pub fn power(variable: SizeId, power: u8) -> Self {
        Self::from_factor((variable, power, 0))
    }

    pub fn from_factor(factor: Factor) -> Self {
        Self::from_terms([Term::of([factor])])
    }

    pub fn is_one(&self) -> bool {
        !self.exp && self.terms.is_empty()
    }

    pub const fn is_exponential(&self) -> bool {
        self.exp
    }

    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    fn from_terms(terms: impl IntoIterator<Item = Term>) -> Self {
        let mut terms: Vec<Term> = terms
            .into_iter()
            .filter(|term| !term.factors.is_empty())
            .collect();
        terms.sort_unstable();
        terms.dedup();
        let survivors = terms
            .iter()
            .filter(|term| {
                !terms
                    .iter()
                    .any(|other| other != *term && other.dominates(term))
            })
            .cloned()
            .collect();
        Self {
            exp: false,
            terms: survivors,
        }
    }

    /// The cost of doing `self` and `other` one after the other, or of whichever is
    /// larger: the dominant growth of the two.
    pub fn max(&self, other: &Self) -> Self {
        if self.exp || other.exp {
            return Self::exponential();
        }
        Self::from_terms(self.terms.iter().chain(&other.terms).cloned())
    }

    /// The cost of doing `other` once for each step of `self`. The constant is part of
    /// every order, so the product also keeps each operand on its own.
    pub fn mul(&self, other: &Self) -> Self {
        if self.exp || other.exp {
            return Self::exponential();
        }
        let products = self
            .terms
            .iter()
            .flat_map(|left| other.terms.iter().map(move |right| left.times(right)));
        Self::from_terms(
            self.terms
                .iter()
                .chain(&other.terms)
                .cloned()
                .chain(products),
        )
    }

    /// The order with `variable` replaced by `replacement`, which is how a callee's
    /// cost is stated in the terms of the caller's sizes.
    pub fn subst(&self, variable: SizeId, replacement: &Self) -> Self {
        self.substitute(&|own| (own == variable).then(|| replacement.clone()))
    }

    /// The order with every variable `replacement` answers for replaced at the same
    /// time, so a replacement is never itself replaced. The terms are pruned once at the
    /// end rather than after each one, which keeps a long order linear to rewrite.
    pub fn substitute(&self, replacement: &dyn Fn(SizeId) -> Option<Self>) -> Self {
        if self.exp {
            return self.clone();
        }
        let mut terms = Vec::new();
        for term in &self.terms {
            let substituted = term.substituted(replacement);
            if substituted.exp {
                return Self::exponential();
            }
            terms.extend(substituted.terms);
        }
        Self::from_terms(terms)
    }

    /// The order with no more than `most` terms, or `None` when it already has no more.
    /// The lightest terms are merged into one that grows at least as fast as each of
    /// them, so the answer is never smaller than the order, only less exact.
    pub fn widened(&self, most: usize) -> Option<Self> {
        if self.terms.len() <= most || most == 0 {
            return None;
        }
        let mut terms = self.terms.clone();
        terms.sort_by_key(|term| std::cmp::Reverse(term.rank()));
        let merged = terms.split_off(most - 1);
        terms.push(Term::bounding(&merged));
        Some(Self::from_terms(terms))
    }

    /// A number that grows with the order, for sorting. Orders that compare as
    /// dominating one another never rank the other way round; incomparable orders
    /// such as `n·m` and `n²` may tie.
    pub fn rank(&self) -> u32 {
        if self.exp {
            return u32::MAX;
        }
        self.terms.iter().map(Term::rank).max().unwrap_or(0)
    }

    /// The order of `log` of a quantity of this order. A logarithm of a product grows
    /// like its largest logarithm, so each term contributes the logarithm of its most
    /// logarithmic factor-variable, and a constant stays constant.
    fn logarithm(&self) -> Self {
        if self.exp {
            return Self::exponential();
        }
        Self::from_terms(
            self.terms
                .iter()
                .flat_map(|term| term.factors.iter())
                .map(|&(variable, _, _)| Term::of([(variable, 0, 1)])),
        )
    }

    fn raised(&self, power: u8) -> Self {
        (0..power).fold(Self::one(), |all, _| all.mul(self))
    }

    /// A wrapper that writes the order with the variable names `naming` gives.
    pub fn named<'a>(&'a self, naming: &'a dyn Fn(SizeId) -> String) -> Named<'a> {
        Named {
            order: self,
            naming,
        }
    }

    /// Reads the grammar [`Display`] writes, resolving each variable name through
    /// `resolve`, which answers `None` for a name it does not know.
    pub fn parse_with(
        text: &str,
        resolve: &mut dyn FnMut(&str) -> Option<SizeId>,
    ) -> Result<Self, ParseOrderError> {
        Reader::new(text, resolve)?.order()
    }
}

impl Term {
    fn substituted(&self, replacement: &dyn Fn(SizeId) -> Option<Order>) -> Order {
        self.factors
            .iter()
            .map(|&(own, power, logarithm)| match replacement(own) {
                None => Order::from_factor((own, power, logarithm)),
                Some(replacement) => replacement
                    .raised(power)
                    .mul(&replacement.logarithm().raised(logarithm)),
            })
            .fold(Order::one(), |all, one| all.mul(&one))
    }

    /// The smallest monomial that grows at least as fast as each of `terms`: every
    /// variable at the largest power and logarithm any of them gives it.
    fn bounding(terms: &[Self]) -> Self {
        let mut largest: std::collections::BTreeMap<SizeId, (u8, u8)> =
            std::collections::BTreeMap::new();
        for &(variable, power, logarithm) in terms.iter().flat_map(|term| &term.factors) {
            let share = largest.entry(variable).or_default();
            *share = (share.0.max(power), share.1.max(logarithm));
        }
        Self::of(
            largest
                .into_iter()
                .map(|(variable, (power, logarithm))| (variable, power, logarithm)),
        )
    }
}

/// The names `n`, `m`, `k`, ... for the first variables, which is what a corpus
/// annotation and a cost table are written in. Beyond them the name is `x` and the
/// index.
const STANDARD_NAMES: [&str; 8] = ["n", "m", "k", "p", "q", "r", "s", "t"];

pub fn standard_name(variable: SizeId) -> String {
    STANDARD_NAMES
        .get(usize::from(variable))
        .map_or_else(|| format!("x{variable}"), |name| (*name).to_owned())
}

fn standard_variable(name: &str) -> Option<SizeId> {
    if let Some(index) = STANDARD_NAMES.iter().position(|known| *known == name) {
        return SizeId::try_from(index).ok();
    }
    let index: SizeId = name.strip_prefix('x')?.parse().ok()?;
    (usize::from(index) >= STANDARD_NAMES.len() && name == standard_name(index)).then_some(index)
}

pub struct Named<'a> {
    order: &'a Order,
    naming: &'a dyn Fn(SizeId) -> String,
}

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.order.exp {
            return f.write_str("exp");
        }
        if self.order.terms.is_empty() {
            return f.write_str("O(1)");
        }
        let terms: Vec<String> = self
            .order
            .terms
            .iter()
            .map(|term| write_term(term, self.naming))
            .collect();
        write!(f, "O({})", terms.join(" + "))
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.named(&standard_name).fmt(f)
    }
}

fn write_term(term: &Term, naming: &dyn Fn(SizeId) -> String) -> String {
    let factors: Vec<String> = term
        .factors
        .iter()
        .map(|&(variable, power, logarithm)| write_factor(&naming(variable), power, logarithm))
        .collect();
    factors.join("\u{b7}")
}

fn write_factor(name: &str, power: u8, logarithm: u8) -> String {
    let mut parts = Vec::new();
    if power > 0 {
        parts.push(format!("{name}{}", superscript(power)));
    }
    match logarithm {
        0 => {}
        1 => parts.push(format!("log {name}")),
        _ => parts.push(format!("log{} {name}", superscript(logarithm))),
    }
    parts.join(" ")
}

fn superscript(power: u8) -> String {
    match power {
        1 => String::new(),
        2 => "\u{b2}".to_owned(),
        3 => "\u{b3}".to_owned(),
        _ => format!("^{power}"),
    }
}

impl FromStr for Order {
    type Err = ParseOrderError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_with(text, &mut standard_variable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOrderError(String);

impl fmt::Display for ParseOrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseOrderError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Name(String),
    Number(u32),
    Caret,
    Superscript(u8),
    Plus,
    Times,
}

fn tokens(text: &str) -> Result<Vec<Token>, ParseOrderError> {
    let mut chars = text.chars().peekable();
    let mut found = Vec::new();
    while let Some(&next) = chars.peek() {
        match next {
            c if c.is_whitespace() => {
                chars.next();
            }
            '^' => {
                chars.next();
                found.push(Token::Caret);
            }
            '+' => {
                chars.next();
                found.push(Token::Plus);
            }
            '*' | '\u{b7}' => {
                chars.next();
                found.push(Token::Times);
            }
            '\u{b2}' => {
                chars.next();
                found.push(Token::Superscript(2));
            }
            '\u{b3}' => {
                chars.next();
                found.push(Token::Superscript(3));
            }
            c if c.is_ascii_digit() => {
                let mut digits = String::new();
                while let Some(digit) = chars.next_if(char::is_ascii_digit) {
                    digits.push(digit);
                }
                let number = digits
                    .parse()
                    .map_err(|_| ParseOrderError(format!("`{digits}` is too large")))?;
                found.push(Token::Number(number));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(letter) = chars.next_if(|c| c.is_ascii_alphanumeric() || *c == '_') {
                    name.push(letter);
                }
                found.push(Token::Name(name));
            }
            other => return Err(ParseOrderError(format!("unexpected `{other}`"))),
        }
    }
    Ok(found)
}

struct Reader<'a> {
    tokens: Vec<Token>,
    at: usize,
    resolve: &'a mut dyn FnMut(&str) -> Option<SizeId>,
}

impl<'a> Reader<'a> {
    fn new(
        text: &str,
        resolve: &'a mut dyn FnMut(&str) -> Option<SizeId>,
    ) -> Result<Self, ParseOrderError> {
        let body = unwrapped(text.trim());
        Ok(Self {
            tokens: tokens(body)?,
            at: 0,
            resolve,
        })
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn order(&mut self) -> Result<Order, ParseOrderError> {
        if self.tokens == [Token::Name("exp".to_owned())] {
            return Ok(Order::exponential());
        }
        let mut all = self.product()?;
        while self.peek() == Some(&Token::Plus) {
            self.next();
            all = all.max(&self.product()?);
        }
        match self.peek() {
            None => Ok(all),
            Some(token) => Err(ParseOrderError(format!("unexpected {token:?}"))),
        }
    }

    fn product(&mut self) -> Result<Order, ParseOrderError> {
        let mut all = self.atoms()?;
        while self.peek() == Some(&Token::Times) {
            self.next();
            all = all.mul(&self.atoms()?);
        }
        Ok(all)
    }

    /// Atoms written side by side, which multiply: `n log n` is `n` times `log n`.
    fn atoms(&mut self) -> Result<Order, ParseOrderError> {
        let mut all = self.atom()?;
        while matches!(self.peek(), Some(Token::Name(_) | Token::Number(_))) {
            all = all.mul(&self.atom()?);
        }
        Ok(all)
    }

    fn atom(&mut self) -> Result<Order, ParseOrderError> {
        match self.next() {
            Some(Token::Number(1)) => Ok(Order::one()),
            Some(Token::Number(other)) => Err(ParseOrderError(format!(
                "`{other}` is not a size; a constant factor is written as 1 or left out"
            ))),
            Some(Token::Name(name)) if name == "log" => {
                let power = self.power()?;
                let variable = self.variable()?;
                Ok(Order::from_factor((variable, 0, power)))
            }
            Some(Token::Name(name)) => {
                let variable = self.resolved(&name)?;
                let power = self.power()?;
                Ok(Order::from_factor((variable, power, 0)))
            }
            Some(token) => Err(ParseOrderError(format!("unexpected {token:?}"))),
            None => Err(ParseOrderError("an order is missing a term".to_owned())),
        }
    }

    fn variable(&mut self) -> Result<SizeId, ParseOrderError> {
        match self.next() {
            Some(Token::Name(name)) => self.resolved(&name),
            _ => Err(ParseOrderError("`log` is missing its variable".to_owned())),
        }
    }

    fn resolved(&mut self, name: &str) -> Result<SizeId, ParseOrderError> {
        (self.resolve)(name).ok_or_else(|| ParseOrderError(format!("`{name}` names no size")))
    }

    fn power(&mut self) -> Result<u8, ParseOrderError> {
        match self.peek() {
            Some(&Token::Superscript(power)) => {
                self.next();
                Ok(power)
            }
            Some(Token::Caret) => {
                self.next();
                match self.next() {
                    Some(Token::Number(power)) if (1..=255).contains(&power) => {
                        Ok(u8::try_from(power).unwrap_or(u8::MAX))
                    }
                    _ => Err(ParseOrderError(
                        "`^` needs a power from 1 to 255".to_owned(),
                    )),
                }
            }
            _ => Ok(1),
        }
    }
}

/// The `O( ... )` around an order is optional, so a table can write `n log n`.
fn unwrapped(text: &str) -> &str {
    text.strip_prefix("O(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic source of small orders, so a failing case reproduces without a
    /// seed to pass around.
    struct Cases(u64);

    impl Cases {
        fn below(&mut self, bound: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % bound
        }

        fn order(&mut self) -> Order {
            if self.below(20) == 0 {
                return Order::exponential();
            }
            let terms = (0..self.below(4)).map(|_| {
                Term::of((0..=self.below(3)).map(|_| {
                    (
                        self.below(3) as SizeId,
                        self.below(4) as u8,
                        self.below(3) as u8,
                    )
                }))
            });
            Order::from_terms(terms)
        }
    }

    fn cases() -> impl Iterator<Item = (Order, Order, Order)> {
        let mut source = Cases(0x9e37_79b9_7f4a_7c15);
        std::iter::repeat_with(move || (source.order(), source.order(), source.order())).take(600)
    }

    fn parsed(text: &str) -> Order {
        text.parse()
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    #[test]
    fn max_is_idempotent_commutative_and_associative() {
        for (a, b, c) in cases() {
            assert_eq!(a.max(&a), a);
            assert_eq!(a.max(&b), b.max(&a));
            assert_eq!(a.max(&b).max(&c), a.max(&b.max(&c)));
        }
    }

    #[test]
    fn mul_is_commutative_associative_and_has_one_as_identity() {
        for (a, b, c) in cases() {
            assert_eq!(a.mul(&b), b.mul(&a));
            assert_eq!(a.mul(&b).mul(&c), a.mul(&b.mul(&c)), "{a} {b} {c}");
            assert_eq!(a.mul(&Order::one()), a);
        }
    }

    #[test]
    fn mul_distributes_over_max_and_dominates_its_operands() {
        for (a, b, c) in cases() {
            assert_eq!(a.mul(&b.max(&c)), a.mul(&b).max(&a.mul(&c)), "{a} {b} {c}");
            assert_eq!(a.max(&a.mul(&b)), a.mul(&b));
        }
    }

    #[test]
    fn substitution_agrees_with_max_and_mul() {
        for (a, b, replacement) in cases() {
            for variable in 0..3 {
                let subst = |order: &Order| order.subst(variable, &replacement);
                assert_eq!(subst(&a.max(&b)), subst(&a).max(&subst(&b)), "{a} {b}");
                assert_eq!(subst(&a.mul(&b)), subst(&a).mul(&subst(&b)), "{a} {b}");
            }
        }
    }

    /// The replacements name only variables no case uses, so replacing one variable after
    /// another and replacing them all at once must agree.
    #[test]
    fn substituting_at_once_agrees_with_one_variable_at_a_time() {
        let sum = Order::var(8).max(&Order::power(9, 2));
        for (order, _, _) in cases() {
            let stepwise = order.subst(0, &Order::var(7)).subst(1, &sum);
            let at_once = order.substitute(&|variable| match variable {
                0 => Some(Order::var(7)),
                1 => Some(sum.clone()),
                _ => None,
            });
            assert_eq!(at_once, stepwise, "{order}");
        }
    }

    #[test]
    fn a_replacement_is_not_itself_replaced() {
        let swapped = parsed("O(n·m²)").substitute(&|variable| match variable {
            0 => Some(Order::var(1)),
            1 => Some(Order::var(0)),
            _ => None,
        });
        assert_eq!(swapped, parsed("O(n²·m)"));
    }

    /// A widened order holds no more terms than asked and is never smaller than the
    /// order it stands for.
    #[test]
    fn widening_bounds_the_terms_and_never_lowers_the_order() {
        let many = (0..40_u16).fold(Order::one(), |all, variable| {
            all.max(&Order::var(variable).mul(&Order::var(variable + 100)))
        });
        assert_eq!(many.terms().len(), 40);
        for most in [1, 2, 8, 32] {
            let widened = many.widened(most).expect("40 terms is over every cap here");
            assert!(widened.terms().len() <= most, "{most}: {widened}");
            assert_eq!(widened.max(&many), widened, "{most} must dominate");
        }
        assert!(many.widened(40).is_none());
        assert!(Order::exponential().widened(1).is_none());
        for (order, _, _) in cases() {
            if let Some(widened) = order.widened(1) {
                assert_eq!(widened.max(&order), widened, "{order}");
            }
        }
    }

    #[test]
    fn substituting_a_variable_for_itself_changes_nothing() {
        for (order, _, _) in cases() {
            for variable in 0..3 {
                assert_eq!(order.subst(variable, &Order::var(variable)), order);
            }
        }
    }

    #[test]
    fn substitution_replaces_the_variable_in_every_term() {
        let quadratic = parsed("O(n²)");
        assert_eq!(quadratic.subst(0, &parsed("O(m)")), parsed("O(m²)"));
        assert_eq!(quadratic.subst(0, &parsed("O(n·m)")), parsed("O(n²·m²)"));
        assert_eq!(quadratic.subst(0, &Order::one()), Order::one());
        assert_eq!(
            parsed("O(n log n)").subst(0, &parsed("O(m·k)")),
            parsed("O(m·k·log m + m·k·log k)")
        );
        assert_eq!(
            quadratic.subst(0, &Order::exponential()),
            Order::exponential()
        );
    }

    #[test]
    fn rank_never_ranks_a_dominated_order_higher() {
        for (a, b, _) in cases() {
            let larger = a.max(&b);
            assert!(larger.rank() >= a.rank() && larger.rank() >= b.rank());
        }
        let ladder = [
            "O(1)",
            "O(log n)",
            "O(n)",
            "O(n log n)",
            "O(n²)",
            "O(n³)",
            "exp",
        ];
        let ranks: Vec<u32> = ladder.iter().map(|text| parsed(text).rank()).collect();
        assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]), "{ranks:?}");
    }

    #[test]
    fn exp_absorbs_everything() {
        let exp = Order::exponential();
        for (order, _, _) in cases() {
            assert_eq!(order.max(&exp), exp);
            assert_eq!(order.mul(&exp), exp);
        }
        assert_eq!(parsed("O(n²)").max(&exp).to_string(), "exp");
    }

    #[test]
    fn a_larger_power_dominates_but_distinct_variables_do_not() {
        assert_eq!(parsed("O(n²)").max(&parsed("O(n)")), parsed("O(n²)"));
        assert_eq!(
            parsed("O(n log n)").max(&parsed("O(n)")),
            parsed("O(n log n)")
        );
        assert_eq!(parsed("O(n)").max(&parsed("O(log n)")), parsed("O(n)"));
        assert_eq!(parsed("O(n·m)").max(&parsed("O(n)")), parsed("O(n·m)"));
        let incomparable = parsed("O(n²)").max(&parsed("O(n·m)"));
        assert_eq!(incomparable.terms().len(), 2);
        assert_eq!(parsed("O(n)").mul(&parsed("O(n)")), parsed("O(n²)"));
        assert_eq!(parsed("O(n)").mul(&parsed("O(m)")), parsed("O(n·m)"));
        assert_eq!(
            parsed("O(n)").mul(&parsed("O(log n)")),
            parsed("O(n log n)")
        );
    }

    #[test]
    fn display_writes_the_standard_forms() {
        let expected = [
            ("O(1)", "O(1)"),
            ("O(log n)", "O(log n)"),
            ("O(n)", "O(n)"),
            ("O(n log n)", "O(n log n)"),
            ("O(n²)", "O(n²)"),
            ("O(n³)", "O(n³)"),
            ("O(n·m)", "O(n·m)"),
            ("O(n^4)", "O(n^4)"),
            ("exp", "exp"),
            ("O(exp)", "exp"),
        ];
        for (written, shown) in expected {
            assert_eq!(parsed(written).to_string(), shown);
        }
    }

    #[test]
    fn the_ascii_forms_read_the_same_as_the_unicode_ones() {
        assert_eq!(parsed("O(n^2)"), parsed("O(n²)"));
        assert_eq!(parsed("O(n*m)"), parsed("O(n·m)"));
        assert_eq!(parsed("n^2"), parsed("O(n²)"));
        assert_eq!(parsed("log^2 n"), parsed("O(log² n)"));
        assert_eq!(parsed("O(n + m)").terms().len(), 2);
        assert_eq!(parsed("O(n + 1)"), parsed("O(n)"));
    }

    #[test]
    fn display_and_from_str_round_trip() {
        for (order, _, _) in cases() {
            assert_eq!(parsed(&order.to_string()), order, "{order}");
        }
    }

    #[test]
    fn a_supplied_naming_is_used_in_both_directions() {
        let order = parsed("O(n·m²)");
        let naming = |variable: SizeId| ["items", "rows"][usize::from(variable)].to_owned();
        let shown = order.named(&naming).to_string();
        assert_eq!(shown, "O(items·rows²)");
        let mut resolve = |name: &str| match name {
            "items" => Some(0),
            "rows" => Some(1),
            _ => None,
        };
        assert_eq!(Order::parse_with(&shown, &mut resolve), Ok(order));
    }

    #[test]
    fn text_that_is_not_an_order_is_refused_with_a_reason() {
        for bad in [
            "", "O()", "O(n", "n +", "O(z)", "O(2n)", "n^0", "n^", "log", "n @ m", "O(n)) ",
        ] {
            assert!(bad.parse::<Order>().is_err(), "`{bad}` was accepted");
        }
        let error = "O(z)".parse::<Order>().unwrap_err();
        assert_eq!(error.to_string(), "`z` names no size");
    }

    #[test]
    fn the_serialised_form_is_compact_and_reads_back() {
        let order = parsed("O(n² + m)");
        let json = serde_json::to_string(&order).unwrap();
        assert_eq!(json, r#"{"terms":[[[0,2,0]],[[1,1,0]]]}"#);
        assert_eq!(serde_json::from_str::<Order>(&json).unwrap(), order);
        assert_eq!(serde_json::to_string(&Order::one()).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&Order::exponential()).unwrap(),
            r#"{"exp":true}"#
        );
    }
}
