//! Extension of a [`TagOperation`].

use gherkin::tagexpr::TagOperation;
use sealed::sealed;

/// Extension of a [`TagOperation`] allowing to evaluate it.
#[sealed]
pub trait Ext {
    /// Evaluates this [`TagOperation`] for the given `tags`.
    #[must_use]
    fn eval<I, S>(&self, tags: I) -> bool
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S> + Clone;
}

#[sealed]
impl Ext for TagOperation {
    fn eval<I, S>(&self, tags: I) -> bool
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S> + Clone,
    {
        match self {
            Self::And(l, r) => l.eval(tags.clone()) & r.eval(tags),
            Self::Or(l, r) => l.eval(tags.clone()) | r.eval(tags),
            Self::Not(t) => !t.eval(tags),
            Self::Tag(t) => tags.into_iter().any(|tag| tag.as_ref() == t),
        }
    }
}
