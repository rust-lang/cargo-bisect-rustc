// Copyright 2018 The Rust Project Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::collections::BTreeMap;
use std::fmt;

pub fn least_satisfying<T, P>(slice: &[T], mut predicate: P) -> usize
where
    T: fmt::Display + fmt::Debug,
    P: FnMut(&T) -> Satisfies,
{
    let mut cache = BTreeMap::new();
    let mut predicate = |idx: usize| *cache.entry(idx).or_insert_with(|| predicate(&slice[idx]));
    let mut unknown_ranges: Vec<(usize, usize)> = Vec::new();
    // presume that the slice starts with a no
    // this should be tested before call
    let mut rm_no = 0;

    // presume that the slice ends with a yes
    // this should be tested before the call
    let mut lm_yes = slice.len() - 1;

    let mut next = (rm_no + lm_yes) / 2;

    loop {
        // simple case with no unknown ranges
        if rm_no + 1 == lm_yes {
            return lm_yes;
        }
        for (left, right) in unknown_ranges.iter().cloned() {
            // if we're straddling an unknown range, then pretend it doesn't exist
            if rm_no + 1 == left && right + 1 == lm_yes {
                return lm_yes;
            }
            // check if we're checking inside an unknown range and set the next check outside of it
            if left <= next && next <= right {
                if rm_no < left - 1 {
                    next = left - 1;
                } else if right < lm_yes {
                    next = right + 1;
                }
                break;
            }
        }

        let r = predicate(next);
        match r {
            Satisfies::Yes => {
                lm_yes = next;
                next = (rm_no + lm_yes) / 2;
            }
            Satisfies::No => {
                rm_no = next;
                next = (rm_no + lm_yes) / 2;
            }
            Satisfies::Unknown => {
                let mut left = next;
                while left > 0 && predicate(left) == Satisfies::Unknown {
                    left -= 1;
                }
                let mut right = next;
                while right + 1 < slice.len() && predicate(right) == Satisfies::Unknown {
                    right += 1;
                }
                unknown_ranges.push((left + 1, right - 1));
                next = left;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Satisfies::*;
    use super::{least_satisfying, Satisfies};
    use quickcheck::{QuickCheck, TestResult};

    fn prop(xs: Vec<Option<bool>>) -> TestResult {
        let mut satisfies_v = xs.into_iter().map(|o| o.into()).collect::<Vec<Satisfies>>();
        satisfies_v.insert(0, Satisfies::No);
        satisfies_v.push(Satisfies::Yes);

        let mut first_yes = None;
        for (i, &s) in satisfies_v.iter().enumerate() {
            match s {
                Satisfies::Yes if first_yes.is_none() => first_yes = Some(i),
                Satisfies::No if first_yes.is_some() => return TestResult::discard(),
                _ => {}
            }
        }

        let res = least_satisfying(&satisfies_v, |i| *i);
        let exp = first_yes.unwrap();
        TestResult::from_bool(res == exp)
    }

    #[test]
    fn least_satisfying_1() {
        assert_eq!(
            least_satisfying(&[No, Unknown, Unknown, No, Yes],|i| *i),
            4
        );
    }

    #[test]
    fn least_satisfying_2() {
        assert_eq!(
            least_satisfying(&[No, Unknown, Yes, Unknown, Yes], |i| *i),
            2
        );
    }

    #[test]
    fn least_satisfying_3() {
        assert_eq!(least_satisfying(&[No, No, No, No, Yes], |i| *i), 4);
    }

    #[test]
    fn least_satisfying_4() {
        assert_eq!(least_satisfying(&[No, No, Yes, Yes, Yes], |i| *i), 2);
    }

    #[test]
    fn least_satisfying_5() {
        assert_eq!(least_satisfying(&[No, Yes, Yes, Yes, Yes], |i| *i), 1);
    }

    #[test]
    fn least_satisfying_6() {
        assert_eq!(
            least_satisfying(&[No, Yes, Yes, Unknown, Unknown, Yes, Unknown, Yes], |i| *i),
            1
        );
    }

    #[test]
    fn least_satisfying_7() {
        assert_eq!(least_satisfying(&[No, Yes, Unknown, Yes], |i| *i), 1);
    }

    #[test]
    fn least_satisfying_8() {
        assert_eq!(
            least_satisfying(&[No, Unknown, No, No, Unknown, Yes, Yes],|i| *i),
            5
        );
    }

    #[test]
    fn qc_prop() {
        QuickCheck::new().quickcheck(prop as fn(_) -> _);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Satisfies {
    Yes,
    No,
    Unknown,
}

impl fmt::Display for Satisfies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<Option<bool>> for Satisfies {
    fn from(o: Option<bool>) -> Self {
        match o {
            Some(true) => Satisfies::Yes,
            Some(false) => Satisfies::No,
            None => Satisfies::Unknown,
        }
    }
}
