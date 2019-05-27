use super::{Term, TryFrom, TryInto, Variant, WrongBoxError};
use crate::exception;
use crate::immix::Heap;
use core::marker::PhantomData;
use std::cmp::Ordering;
use std::ptr::NonNull;

#[derive(Debug, Eq)]
#[repr(C)]
pub struct Cons {
    pub head: Term,
    pub tail: Term,
}

unsafe impl Sync for Cons {}

impl PartialEq for Cons {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl PartialOrd for Cons {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cons {
    /// Lists are compared element by element.
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

// TODO: to be TryFrom once rust stabilizes the trait
impl TryFrom<Term> for Cons {
    type Error = WrongBoxError;

    #[inline]
    fn try_from(value: &Term) -> Result<&Self, WrongBoxError> {
        if let Variant::Cons(ptr) = value.into_variant() {
            unsafe { return Ok(&*(ptr as *const Cons)) }
        }
        Err(WrongBoxError)
    }
}

pub struct Iter<'a> {
    head: Option<NonNull<Cons>>,
    //len: usize,
    marker: PhantomData<&'a Cons>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Term;

    #[inline]
    fn next(&mut self) -> Option<&'a Term> {
        self.head.map(|node| unsafe {
            // Need an unbound lifetime to get 'a
            let node = &*node.as_ptr();
            if let Ok(cons) = node.tail.try_into() {
                self.head = Some(NonNull::new_unchecked(cons as *const Cons as *mut Cons));
            } else {
                // TODO match badly formed lists
                self.head = None;
            }
            &node.head
        })
    }
}

impl Cons {
    pub fn iter(&self) -> Iter {
        Iter {
            head: unsafe { Some(NonNull::new_unchecked(self as *const Cons as *mut Cons)) },
            //len: self.len,
            marker: PhantomData,
        }
    }
}

impl<'a> IntoIterator for &'a Cons {
    type Item = &'a Term;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl Cons {
    // pub fn from_iter<I: IntoIterator<Item=Term>>(iter: I, heap: &Heap) -> Self
    //     where I::Item: DoubleEndedIterator {
    //     iter.rev().fold(Term::nil(), |res, val| value::cons(heap, val, res))
    // }

    // impl FromIterator<Term> for Cons { can't do this since we need heap

    pub fn from_iter<I: IntoIterator<Item = Term> + ExactSizeIterator>(
        iter: I,
        heap: &Heap,
    ) -> Term {
        let len = iter.len();
        // TODO: maybe just mut iter in the header
        let mut iter = iter.into_iter();
        if let Some(val) = iter.next() {
            let c = heap.alloc(Cons {
                head: val,
                tail: Term::nil(),
            });

            unsafe {
                (0..len - 1).fold(c as *mut Cons, |cons, _i| {
                    let Cons { ref mut tail, .. } = *cons;
                    let val = iter.next().unwrap();
                    let new_cons = heap.alloc(Cons {
                        head: val,
                        tail: Term::nil(),
                    });
                    let ptr = new_cons as *mut Cons;
                    std::mem::replace(&mut *tail, Term::from(new_cons));
                    ptr
                });
            }

            Term::from(c)
        } else {
            Term::nil()
        }
    }
}

/// @brief Fill buf with the UTF8 contents of the unicode list
/// @param len Max number of characters to write.
/// @param written NULL or bytes written.
/// @return 0 ok,
///        -1 type error,
///        -2 list too long, only \c len characters written
pub fn unicode_list_to_buf(list: &Cons, _max_len: usize) -> Result<String, exception::Exception> {
    // TODO: handle max_len
    list.iter()
        .map(|v| {
            v.to_uint()
                .and_then(std::char::from_u32)
                .ok_or_else(|| exception::Exception::new(exception::Reason::EXC_BADARG))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;

    #[test]
    fn test_from_iter() {
        let heap = Heap::new();

        let tup = tup3!(&heap, Term::int(1), Term::int(2), Term::int(3));
        let t: &value::Tuple = tup.try_into().expect("wasn't a tuple");

        let res = Cons::from_iter(t.into_iter().cloned(), &heap);
        let cons: &Cons = res.try_into().expect("wasn't a cons");

        let mut iter = cons.iter();
        assert_eq!(Some(&Term::int(1)), iter.next());
        assert_eq!(Some(&Term::int(2)), iter.next());
        assert_eq!(Some(&Term::int(3)), iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_from_empty_iter() {
        let heap = Heap::new();

        let items = vec![];

        let res = Cons::from_iter(items.into_iter(), &heap);
        assert!(res.is_nil());
    }

    #[test]
    fn test_unicode_list_to_buf() {
        let _heap = Heap::new();

        // '関数に渡すことで'
        // [38306, 25968, 12395, 28193, 12377, 12371, 12392, 12391]
    }
}
