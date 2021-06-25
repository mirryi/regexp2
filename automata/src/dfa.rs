use crate::matching::Match;
use crate::table::Table;

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

/// A deterministic finite automaton, or DFA.
#[derive(Debug, Clone)]
pub struct DFA<T>
where
    T: Clone + Eq + Hash,
{
    /// A DFA has a single initial state.
    pub initial_state: usize,
    /// The number of total states in the DFA. There is a state labeled i for every i where 0 <= i
    /// < total_states.
    pub total_states: usize,
    /// The set of accepting states.
    pub final_states: HashSet<usize>,
    /// A lookup table for transitions between states.
    pub transition: Table<usize, Transition<T>, usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Transition<T>(pub T)
where
    T: Clone + Eq + Hash;

impl<T> DFA<T>
where
    T: Clone + Eq + Hash,
{
    /// Create a new DFA with a single initial state.
    #[inline]
    pub fn new() -> Self {
        Self {
            initial_state: 0,
            total_states: 1,
            final_states: HashSet::new(),
            transition: Table::new(),
        }
    }
}

impl<T> Default for DFA<T>
where
    T: Clone + Eq + Hash,
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> DFA<T>
where
    T: Clone + Eq + Hash,
{
    #[inline]
    pub fn add_state(&mut self, is_final: bool) -> usize {
        let label = self.total_states;
        self.total_states += 1;
        if is_final {
            self.final_states.insert(label);
        }
        label
    }

    #[inline]
    pub fn add_transition(&mut self, start: usize, end: usize, label: Transition<T>) -> Option<()> {
        if self.total_states < start + 1 || self.total_states < end + 1 {
            None
        } else {
            self.transition.set(start, label, end);
            Some(())
        }
    }

    #[inline]
    pub fn transitions_on(&self, state: &usize) -> HashMap<&Transition<T>, &usize> {
        self.transition.get_row(state)
    }

    #[inline]
    pub fn is_final_state(&self, state: &usize) -> bool {
        self.final_states.iter().any(|s| s == state)
    }
}

impl<T> DFA<T>
where
    T: Clone + Eq + Hash,
{
    #[inline]
    pub fn iter_on<I>(&self, input: I) -> Iter<'_, T, I::IntoIter>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        Iter {
            dfa: &self,

            input: input.into_iter(),
            current: self.initial_state,
        }
    }

    #[inline]
    pub fn into_iter_on<I>(self, input: I) -> IntoIter<T, I::IntoIter>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        let current = self.initial_state;
        IntoIter {
            dfa: self,

            input: input.into_iter(),
            current,
        }
    }
}

#[derive(Debug)]
pub struct Iter<'a, T, I>
where
    T: Clone + Eq + Hash,
    T: PartialEq<I::Item>,
    I: Iterator,
{
    dfa: &'a DFA<T>,

    input: I,
    current: usize,
}

impl<'a, T, I> Iterator for Iter<'a, T, I>
where
    T: Clone + Eq + Hash,
    T: PartialEq<I::Item>,
    I: Iterator,
{
    type Item = (usize, I::Item, bool);

    fn next(&mut self) -> Option<Self::Item> {
        iter_on_next(&self.dfa, &mut self.input, &mut self.current)
    }
}

#[derive(Debug)]
pub struct IntoIter<T, I>
where
    T: Clone + Eq + Hash,
    T: PartialEq<I::Item>,
    I: Iterator,
{
    dfa: DFA<T>,

    input: I,
    current: usize,
}

impl<T, I> Iterator for IntoIter<T, I>
where
    T: Clone + Eq + Hash,
    T: PartialEq<I::Item>,
    I: Iterator,
{
    type Item = (usize, I::Item, bool);

    fn next(&mut self) -> Option<Self::Item> {
        iter_on_next(&self.dfa, &mut self.input, &mut self.current)
    }
}

#[inline]
fn iter_on_next<T, I>(
    dfa: &DFA<T>,
    input: &mut I,
    current: &mut usize,
) -> Option<(usize, I::Item, bool)>
where
    T: Clone + Eq + Hash,
    T: PartialEq<I::Item>,
    I: Iterator,
{
    let state = *current;
    let is = match input.next() {
        Some(v) => v,
        None => return None,
    };

    let transitions = dfa.transitions_on(&state);
    let next_state = match transitions.iter().find(|(&Transition(t), _)| *t == is) {
        Some((_, &&s)) => s,
        None => return None,
    };

    let is_final = dfa.is_final_state(&next_state);

    *current = next_state;
    Some((next_state, is, is_final))
}

impl<T> DFA<T>
where
    T: Clone + Eq + Hash,
{
    /// Determine if the given input is accepted by the DFA.
    #[inline]
    pub fn is_match<I>(&self, input: I) -> bool
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        match self.iter_on(input).last() {
            Some((_, _, is_final)) => is_final,
            None => false,
        }
    }

    #[inline]
    pub fn find_shortest<I>(&self, input: I) -> Option<(Match<I::Item>, usize)>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        self.find_shortest_at(input, 0)
    }

    #[inline]
    pub fn find_shortest_at<I>(&self, input: I, start: usize) -> Option<(Match<I::Item>, usize)>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        self.find_at_impl(input, start, true)
    }

    #[inline]
    pub fn find<I>(&self, input: I) -> Option<(Match<I::Item>, usize)>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        self.find_at(input, 0)
    }

    #[inline]
    pub fn find_at<I>(&self, input: I, start: usize) -> Option<(Match<I::Item>, usize)>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        self.find_at_impl(input, start, false)
    }

    #[inline]
    fn find_at_impl<I>(
        &self,
        input: I,
        start: usize,
        shortest: bool,
    ) -> Option<(Match<I::Item>, usize)>
    where
        T: PartialEq<I::Item>,
        I: IntoIterator,
    {
        let mut last_match = if self.is_final_state(&self.initial_state) {
            Some(Match::new(start, start, vec![]))
        } else {
            None
        };

        let mut state = self.initial_state;
        if !(shortest && last_match.is_some()) {
            let iter = self.iter_on(input).skip(start).enumerate();

            let mut span = Vec::new();
            for (i, (s, is, is_final)) in iter {
                let is_rc = Rc::new(is);
                span.push(is_rc);

                state = s;

                if is_final {
                    last_match = Some(Match::new(start, i + 1, span.clone()));
                    if shortest {
                        break;
                    }
                }
            }
        }

        last_match.map(|m| {
            (
                Match::new(
                    m.start,
                    m.end,
                    m.span
                        .into_iter()
                        .map(|rc| match Rc::try_unwrap(rc) {
                            Ok(v) => v,
                            // Shouldn't ever have any lingering references.
                            Err(_) => unreachable!("MatchRc somehow had lingering references"),
                        })
                        .collect(),
                ),
                state,
            )
        })
    }
}
