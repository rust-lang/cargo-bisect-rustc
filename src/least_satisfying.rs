use std::collections::BTreeMap;
use std::fmt;

// slice is the slice of values to be tested
// start_offset is the offset of the start of the given `slice`
//   inside the bigger "true" slice of all the possible values.
// predicate receives the value from the `slice`, the (estimate) amount of
//   values left to test, and an estimate of the steps left
//
// Returns the index of the earliest element that Satisfies::Yes the predicate.
pub fn least_satisfying<T, P>(slice: &[T], start_offset: usize, mut predicate: P) -> usize
where
    T: fmt::Display + fmt::Debug,
    P: FnMut(&T, usize, usize) -> Satisfies,
{
    let mut cache = BTreeMap::new();
    let mut predicate = |idx: usize, rm_no, lm_yes| {
        let range: usize = lm_yes - rm_no + 1;
        let remaining = range / 2;

        let estimate;
        {
            // The estimate of the remaining step count based on the range of the values left to check.
            // Can be an underestimate if the (future) midpoint(s) don't land close enough to the
            // true middle of the bisected ranges, but usually by no more than 2.
            let range_est = range.ilog2() as usize;
            // The estimate of the remaining step count based on the height of the current idx in
            // the overall binary tree. This is tailored to the specific midpoint selection strategy
            // currently used, and relies on the fact that each step of the way we get at least
            // one more step away from the root of the binary tree.
            // Can arbitrarily overestimate the number of steps (think a short bisection range centered
            // around the tree root).
            // Can also *under*estimate the number of steps if the `idx` was not actually
            // a direct result of `midpoint_stable_offset`, but rather tweaked slightly to work around
            // unknown ranges.
            let height_est = (start_offset + 1 + idx).trailing_zeros() as usize;
            // Real estimate. Combines our best guesses via the two above methods. Can still be somewhat
            // off in presence of unknown ranges.
            estimate = height_est.clamp(range_est, range_est + 2);
        }
        *cache
            .entry(idx)
            .or_insert_with(|| predicate(&slice[idx], remaining, estimate))
    };
    let mut unknown_ranges: Vec<(usize, usize)> = Vec::new();
    // presume that the slice starts with a no
    // this should be tested before call
    let mut rm_no = 0;

    // presume that the slice ends with a yes
    // this should be tested before the call
    let mut lm_yes = slice.len() - 1;

    let mut next: usize;
    loop {
        // simple case with no unknown ranges
        if rm_no + 1 == lm_yes {
            return lm_yes;
        }
        next = midpoint_stable_offset(start_offset, rm_no, lm_yes);

        for (left, right) in unknown_ranges.iter().copied() {
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

        let r = predicate(next, rm_no, lm_yes);
        match r {
            Satisfies::Yes => {
                lm_yes = next;
            }
            Satisfies::No => {
                rm_no = next;
            }
            Satisfies::Unknown => {
                let mut left = next;
                while left > 0 && predicate(left, rm_no, lm_yes) == Satisfies::Unknown {
                    left -= 1;
                }
                let mut right = next;
                while right + 1 < slice.len()
                    && predicate(right, rm_no, lm_yes) == Satisfies::Unknown
                {
                    right += 1;
                }
                unknown_ranges.push((left + 1, right - 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::midpoint_stable;
    use super::Satisfies::{No, Unknown, Yes};
    use super::{least_satisfying, Satisfies};
    use quickcheck::{QuickCheck, TestResult};

    fn prop(offset: usize, xs: Vec<Option<bool>>) -> TestResult {
        let mut satisfies_v = xs
            .into_iter()
            .map(std::convert::Into::into)
            .collect::<Vec<Satisfies>>();
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
        if offset > usize::MAX / 2 {
            // not interested in testing usize overflows
            return TestResult::discard();
        }

        let res = least_satisfying(&satisfies_v, offset, |i, _, _| *i);
        let exp = first_yes.unwrap();
        TestResult::from_bool(res == exp)
    }

    #[test]
    fn least_satisfying_1() {
        assert_eq!(
            least_satisfying(&[No, Unknown, Unknown, No, Yes], 0, |i, _, _| *i),
            4
        );
    }

    #[test]
    fn least_satisfying_2() {
        assert_eq!(
            least_satisfying(&[No, Unknown, Yes, Unknown, Yes], 0, |i, _, _| *i),
            2
        );
    }

    #[test]
    fn least_satisfying_3() {
        assert_eq!(least_satisfying(&[No, No, No, No, Yes], 0, |i, _, _| *i), 4);
    }

    #[test]
    fn least_satisfying_4() {
        assert_eq!(
            least_satisfying(&[No, No, Yes, Yes, Yes], 0, |i, _, _| *i),
            2
        );
    }

    #[test]
    fn least_satisfying_5() {
        assert_eq!(
            least_satisfying(&[No, Yes, Yes, Yes, Yes], 0, |i, _, _| *i),
            1
        );
    }

    #[test]
    fn least_satisfying_6() {
        assert_eq!(
            least_satisfying(
                &[No, Yes, Yes, Unknown, Unknown, Yes, Unknown, Yes],
                0,
                |i, _, _| *i
            ),
            1
        );
    }

    #[test]
    fn least_satisfying_7() {
        assert_eq!(
            least_satisfying(&[No, Yes, Unknown, Yes], 0, |i, _, _| *i),
            1
        );
    }

    #[test]
    fn least_satisfying_8() {
        assert_eq!(
            least_satisfying(&[No, Unknown, No, No, Unknown, Yes, Yes], 0, |i, _, _| *i),
            5
        );
    }

    #[test]
    fn least_satisfying_9() {
        assert_eq!(least_satisfying(&[No, Unknown, Yes], 0, |i, _, _| *i), 2);
    }

    #[test]
    fn qc_prop_least_satisfying() {
        QuickCheck::new().quickcheck(prop as fn(_, _) -> _);
    }

    #[test]
    fn midpoint_test() {
        assert_eq!(midpoint_stable(1, 3), 2);
        assert_eq!(midpoint_stable(3, 6), 4);
        assert_eq!(midpoint_stable(1, 5), 4);
        assert_eq!(midpoint_stable(2, 5), 4);
        assert_eq!(midpoint_stable(4, 7), 6);
        assert_eq!(midpoint_stable(8, 13), 12);
        assert_eq!(midpoint_stable(8, 16), 12);

        assert_eq!(midpoint_stable(25, 27), 26);
        assert_eq!(midpoint_stable(25, 28), 26);
        assert_eq!(midpoint_stable(25, 29), 28);
        assert_eq!(midpoint_stable(33, 65), 64);
    }

    #[test]
    fn qc_prop_midpoint_stable() {
        fn prop_midpoint(left: usize, right: usize) -> TestResult {
            if left > usize::MAX / 2 || right > usize::MAX / 2 {
                return TestResult::discard();
            }
            if left == 0 {
                return TestResult::discard();
            }
            if left + 1 >= right {
                return TestResult::discard();
            }
            let mid = midpoint_stable(left, right);
            // check that it's in range
            if mid <= left || right <= mid {
                return TestResult::failed();
            }
            // check that there are no less-deep candidates in range
            let mid_height = mid.trailing_zeros();
            let step = 1 << (mid_height + 1);
            let mut probe = left & !(step - 1);
            while probe < right {
                if probe > left {
                    return TestResult::failed();
                }
                probe += step;
            }
            TestResult::passed()
        }
        QuickCheck::new().quickcheck(prop_midpoint as fn(_, _) -> _);
    }
}

// see documentation of `midpoint_stable` below
fn midpoint_stable_offset(start_offset: usize, left: usize, right: usize) -> usize {
    // return (left + right)/2;
    // The implementation of `midpoint_stable` treats the slice as a binary tree
    // with the assumption that the slice index starts at one, not zero
    // (i.e. it assumes that both 1 and 3 are child nodes of 2, and 0 is not present
    // in the tree at all).
    // But we don't want to bubble this requirement up the stack since it's a bit
    // counterintuitive and hard to explain, so just bump it here instead
    let start_offset = start_offset + 1;
    midpoint_stable(left + start_offset, right + start_offset) - start_offset
}
/// Returns a "stabilized midpoint" between the two slice indices (endpoints excluded).
///
/// That is, returns such an index that is likely to be reused by future bisector invocations.
/// In practice, this reinterprets the slice as a "complete" (i.e. left-heavy) binary tree,
/// and finds the lowest-depth node between the two indices. This ensures that low-depth
/// nodes are more likely to be tried first (and thus reused) regardless of the initial search boundaries,
/// while still keeping the "binary" in "binary search" and completing the task in O(log_2(n)) steps
fn midpoint_stable(left: usize, right: usize) -> usize {
    assert!(
        (right - left) > 1,
        "midpoint_stable called with consecutive values. Can't handle this, there's no midpoint. {:?} vs {:?}",
        left,
        right
    );
    // If we only have a single candidate - return it
    if left + 1 == right - 1 {
        return left + 1;
    }

    // If left and right have the same binary digits up to nth place,
    //   left  = 0bxxx0yyyy;
    //   right = 0bxxx1zzzz;
    // then we have a number of the form
    //   mid   = 0bxxx10000;
    // which has the least possible depth (as indicated by the amount of trailing zeroes)
    // of all the numbers between left (exclusive) and right (inclusive).
    // The following code constructs said number (with the exception that it excludes the right bound)
    let diff = isolate_most_significant_one(left ^ (right - 1));
    assert!(left & diff == 0);
    assert!((right - 1) & diff > 0);
    // grab the high bits from left_next, force 1 where it should be, and zero out the lower bits.
    let mask = !(diff - 1);
    let mid = (mask & left) | diff;
    return mid;
}

// Implementation copy-pasted from std nightly `feature(isolate_most_significant_one)`
// https://github.com/rust-lang/rust/pull/136910
const fn isolate_most_significant_one(x: usize) -> usize {
    x & (((1 as usize) << (<usize>::BITS - 1)).wrapping_shr(x.leading_zeros()))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Satisfies {
    Yes,
    No,
    Unknown,
}

impl Satisfies {
    pub fn msg_with_context<'a>(&self, term_old: &'a str, term_new: &'a str) -> &'a str {
        match self {
            Self::Yes => term_new,
            Self::No => term_old,
            Self::Unknown => "Unable to figure out if the condition matched",
        }
    }
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
