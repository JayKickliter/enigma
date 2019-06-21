use crate::atom;
use crate::bif;
use crate::bitstring::{self, Binary, RcBinary, SubBinary};
use crate::exception::Exception;
use crate::process::RcProcess;
use crate::value::{self, CastFrom, CastInto, Cons, Term, Tuple, Variant};
use crate::vm;

pub fn split_binary_2(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    // bin, pos
    let heap = &process.context_mut().heap;

    let pos = match args[1].into_variant() {
        Variant::Integer(i) if i >= 0 => i as usize,
        _ => return Err(badarg!()),
    };

    if !args[0].is_binary() {
        return Err(badarg!());
    }

    // TODO: this was a get_real_binary macro before
    let (bin, offset, bit_offset, size, bitsize) = match args[0].get_boxed_header() {
        Ok(value::BOXED_BINARY) => {
            let value = &args[0].get_boxed_value::<bitstring::RcBinary>().unwrap();
            (*value, 0, 0, value.data.len(), 0)
        }
        Ok(value::BOXED_SUBBINARY) => {
            let value = &args[0].get_boxed_value::<bitstring::SubBinary>().unwrap();
            (
                &value.original,
                value.offset,
                value.bit_offset,
                value.size,
                value.bitsize,
            )
        }
        _ => unreachable!(),
    };

    if size < pos {
        return Err(badarg!());
    }

    let sb1 = bitstring::SubBinary {
        original: bin.clone(),
        size: pos,
        offset: offset + pos,
        bit_offset,
        bitsize: 0,
        is_writable: false,
    };

    let sb2 = bitstring::SubBinary {
        original: bin.clone(),
        size: size - pos,
        offset: offset + pos,
        bit_offset,
        bitsize, // The extra bits go into the second binary.
        is_writable: false,
    };

    Ok(tup2!(
        heap,
        Term::subbinary(heap, sb1),
        Term::subbinary(heap, sb2)
    ))
}

fn part(source: Term, mut pos: usize, len: isize) -> Result<SubBinary, Exception> {
    let (bin, offs, bitoffs, size, bitsize) = match source.get_boxed_header() {
        Ok(value::BOXED_BINARY) => {
            let value = &source.get_boxed_value::<RcBinary>().unwrap();
            (*value, 0, 0, value.data.len(), 0)
        }
        Ok(value::BOXED_SUBBINARY) => {
            let value = &source.get_boxed_value::<SubBinary>().unwrap();
            (
                &value.original,
                value.offset,
                value.bit_offset,
                value.size,
                value.bitsize,
            )
        }
        _ => return Err(badarg!()),
    };

    let len = if len < 0 {
        let len = (-len) as usize;
        if len > pos {
            return Err(badarg!());
        }
        pos -= len;
        len
    } else {
        len as usize
    };

    /* overflow */
    // if ((pos + len) < pos || (len > 0 && (pos + len) == pos) {
    // goto badarg;
    // }
    if size < pos || size < (pos + len) {
        return Err(badarg!());
    }

    // TODO: make a constructor that doesn't need bits.
    let offset = (offs * 8) + bitoffs as usize + (pos * 8);
    let size = len * 8;

    // TODO: tests
    Ok(SubBinary::new(bin.clone(), size, offset, false))
}

pub fn part_2(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    // PosLen = {Start :: integer() >= 0, Length :: integer()}
    let source = args[0];

    if let Ok(tup) = Tuple::cast_from(&args[1]) {
        if tup.len != 2 {
            return Err(badarg!());
        }

        let pos = match tup[0].into_variant() {
            Variant::Integer(i) if i >= 0 => i as usize,
            _ => return Err(badarg!()),
        };

        let len = match tup[1].into_variant() {
            Variant::Integer(i) => i as isize,
            _ => return Err(badarg!()),
        };

        let heap = &process.context_mut().heap;
        let subbin = part(source, pos, len)?;
        return Ok(Term::subbinary(heap, subbin));
    }

    Err(badarg!())
}

pub fn part_3(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    let source = args[0];

    let pos = match args[1].into_variant() {
        Variant::Integer(i) if i >= 0 => i as usize,
        _ => return Err(badarg!()),
    };

    let len = match args[2].into_variant() {
        Variant::Integer(i) => i as isize,
        _ => return Err(badarg!()),
    };

    let heap = &process.context_mut().heap;
    let subbin = part(source, pos, len)?;
    Ok(Term::subbinary(heap, subbin))
}

// TODO: share some of the impl with split
pub fn compile_pattern_1(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    use regex::bytes::Regex;
    let heap = &process.context_mut().heap;

    // pattern = binary | [binary] | compiled
    let regex = if let Some(bytes) = args[0].to_bytes() {
        let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
        let regex = Regex::new(&pattern).unwrap();
        regex
    } else if args[0].is_list() {
        let mut iter = args[0];
        let mut acc = Vec::new();
        while let Ok(Cons { head, tail }) = Cons::cast_from(&iter) {
            // TODO: error handling
            let bytes = head.to_bytes().unwrap();
            let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
            acc.push(pattern);
            iter = *tail;
        }

        if !iter.is_nil() {
            return Err(badarg!());
        }

        let pattern = acc.join("|");
        let regex = Regex::new(&pattern).unwrap();
        regex
    } else {
        return Err(badarg!());
    };

    Ok(Term::regex(heap, regex))
}

pub fn split_2(vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    split_3(vm, process, &[args[0], args[1], Term::nil()])
}

// TODO: split on "" is invalid
pub fn split_3(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    use regex::bytes::Regex;
    use std::borrow::Cow;
    let heap = &process.context_mut().heap;
    // <subject> <pattern> <options>
    // split or replace via regex crate and regex::escape the contents. It'll pick the most
    // efficient one.

    // subject = binary
    let (bin, offs, bitoffs, size, bitsize) = match args[0].get_boxed_header() {
        Ok(value::BOXED_BINARY) => {
            let value = &args[0].get_boxed_value::<RcBinary>().unwrap();
            (*value, 0, 0, value.data.len(), 0)
        }
        Ok(value::BOXED_SUBBINARY) => {
            let value = &args[0].get_boxed_value::<SubBinary>().unwrap();
            (
                &value.original,
                value.offset,
                value.bit_offset,
                value.size,
                value.bitsize,
            )
        }
        _ => unreachable!(),
    };
    if bitoffs > 0 {
        unimplemented!("Unaligned bitoffs not implemented");
    }
    let subject = &bin.data[offs..offs + size];

    // pattern = binary | [binary] | compiled
    let regex = if let Ok(regex) = Regex::cast_from(&args[1]) {
        Cow::Borrowed(regex)
    } else if let Some(bytes) = args[1].to_bytes() {
        let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else if args[1].is_list() {
        let mut iter = args[1];
        let mut acc = Vec::new();
        while let Ok(Cons { head, tail }) = Cons::cast_from(&iter) {
            // TODO: error handling
            let bytes = head.to_bytes().unwrap();
            let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
            acc.push(pattern);
            iter = *tail;
        }

        if !iter.is_nil() {
            return Err(badarg!());
        }

        let pattern = acc.join("|");
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else {
        return Err(badarg!());
    };

    let mut global = false;

    // parse options
    if let Ok(cons) = Cons::cast_from(&args[2]) {
        for val in cons.iter() {
            match val.into_variant() {
                Variant::Atom(atom::TRIM) => {
                    // remove empty trailing parts
                    unimplemented!()
                }
                Variant::Atom(atom::TRIM_ALL) => {
                    // remove all empty parts
                    unimplemented!()
                }
                Variant::Atom(atom::GLOBAL) => {
                    // repeat globally
                    global = true;
                }
                Variant::Pointer(..) => {
                    if let Ok(tup) = Tuple::cast_from(&args[2]) {
                        if tup.len != 2 {
                            return Err(badarg!());
                        }

                        match tup[0].into_variant() {
                            Variant::Atom(atom::SCOPE) => unimplemented!(),
                            _ => return Err(badarg!()),
                        }
                    } else {
                        return Err(badarg!());
                    }
                }
                _ => return Err(badarg!()),
            }
        }
    } else if args[2].is_nil() {
        // skip
    } else {
        return Err(badarg!());
    }

    if global {
        let mut finder = regex.find_iter(subject);
        let mut last = 0;
        let mut acc = Vec::new();

        loop {
            // based on regex split code, but we needed offsets instead of slices
            match finder.next() {
                None => {
                    if last >= subject.len() {
                        break;
                    } else {
                        acc.push(SubBinary::new(
                            bin.clone(),
                            (subject.len() - last) * 8,
                            (offs + last) * 8,
                            false,
                        ));

                        last = subject.len();
                    }
                }
                Some(m) => {
                    acc.push(SubBinary::new(
                        bin.clone(),
                        (m.start() - last) * 8,
                        (offs + last) * 8,
                        false,
                    ));
                    last = m.end();
                }
            }
        }

        let res = acc.into_iter().rev().fold(Term::nil(), |acc, val| {
            cons!(heap, Term::subbinary(heap, val), acc)
        });
        Ok(res)
    } else {
        let res = match regex.find(subject) {
            None => cons!(heap, args[0], Term::nil()),
            Some(m) => {
                let s1 = SubBinary::new(bin.clone(), m.start() * 8, offs * 8, false);
                let s2 = SubBinary::new(
                    bin.clone(),
                    (subject.len() - m.end()) * 8,
                    (offs + m.end()) * 8,
                    false,
                );
                cons!(
                    heap,
                    Term::subbinary(heap, s1),
                    cons!(heap, Term::subbinary(heap, s2), Term::nil())
                )
            }
        };
        Ok(res)
    }
}

pub fn match_2(vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    match_3(vm, process, &[args[0], args[1], Term::nil()])
}

// mostly identical to matches/3
pub fn match_3(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    use regex::bytes::Regex;
    use std::borrow::Cow;
    let heap = &process.context_mut().heap;
    // <subject> <pattern> <options>

    // subject = binary
    let subject = match args[0].to_bytes() {
        Some(bytes) => bytes,
        None => return Err(badarg!()),
    };

    // pattern = binary | [binary] | compiled
    let regex = if let Ok(regex) = Regex::cast_from(&args[1]) {
        Cow::Borrowed(regex)
    } else if let Some(bytes) = args[1].to_bytes() {
        let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else if args[1].is_list() {
        let mut iter = args[1];
        let mut acc = Vec::new();
        while let Ok(Cons { head, tail }) = Cons::cast_from(&iter) {
            // TODO: error handling
            let bytes = head.to_bytes().unwrap();
            let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
            acc.push(pattern);
            iter = *tail;
        }

        if !iter.is_nil() {
            return Err(badarg!());
        }

        let pattern = acc.join("|");
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else {
        return Err(badarg!());
    };

    // parse options
    if let Ok(cons) = Cons::cast_from(&args[2]) {
        for val in cons.iter() {
            match val.into_variant() {
                Variant::Pointer(..) => {
                    if let Ok(tup) = Tuple::cast_from(&args[2]) {
                        if tup.len != 2 {
                            return Err(badarg!());
                        }

                        match tup[0].into_variant() {
                            Variant::Atom(atom::SCOPE) => unimplemented!(),
                            _ => return Err(badarg!()),
                        }
                    } else {
                        return Err(badarg!());
                    }
                }
                _ => return Err(badarg!()),
            }
        }
    } else if args[2].is_nil() {
        // skip
    } else {
        return Err(badarg!());
    }

    let res = regex
        .find(subject)
        .map(|m| {
            tup2!(
                heap,
                Term::uint64(heap, m.start() as u64),
                Term::uint64(heap, (m.end() - m.start()) as u64)
            )
        })
        .unwrap_or_else(|| atom!(NOMATCH));

    Ok(res)
}

pub fn matches_2(vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    matches_3(vm, process, &[args[0], args[1], Term::nil()])
}

// very similar to split: extract helpers
pub fn matches_3(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    use regex::bytes::Regex;
    use std::borrow::Cow;
    let heap = &process.context_mut().heap;
    // <subject> <pattern> <options>

    // subject = binary
    let subject = match args[0].to_bytes() {
        Some(bytes) => bytes,
        None => return Err(badarg!()),
    };

    // pattern = binary | [binary] | compiled
    let regex = if let Ok(regex) = Regex::cast_from(&args[1]) {
        Cow::Borrowed(regex)
    } else if let Some(bytes) = args[1].to_bytes() {
        let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else if args[1].is_list() {
        let mut iter = args[1];
        let mut acc = Vec::new();
        while let Ok(Cons { head, tail }) = Cons::cast_from(&iter) {
            // TODO: error handling
            let bytes = head.to_bytes().unwrap();
            let pattern = regex::escape(std::str::from_utf8(bytes).unwrap());
            acc.push(pattern);
            iter = *tail;
        }

        if !iter.is_nil() {
            return Err(badarg!());
        }

        let pattern = acc.join("|");
        let regex = Regex::new(&pattern).unwrap();
        Cow::Owned(regex)
    } else {
        return Err(badarg!());
    };

    // parse options
    if let Ok(cons) = Cons::cast_from(&args[2]) {
        for val in cons.iter() {
            match val.into_variant() {
                Variant::Pointer(..) => {
                    if let Ok(tup) = Tuple::cast_from(&args[2]) {
                        if tup.len != 2 {
                            return Err(badarg!());
                        }

                        match tup[0].into_variant() {
                            Variant::Atom(atom::SCOPE) => unimplemented!(),
                            _ => return Err(badarg!()),
                        }
                    } else {
                        return Err(badarg!());
                    }
                }
                _ => return Err(badarg!()),
            }
        }
    } else if args[2].is_nil() {
        // skip
    } else {
        return Err(badarg!());
    }

    let values: Vec<_> = regex
        .find_iter(subject)
        .map(|m| {
            tup2!(
                heap,
                Term::uint64(heap, m.start() as u64),
                Term::uint64(heap, (m.end() - m.start()) as u64)
            )
        })
        .collect();
    let res = values
        .into_iter()
        .rev()
        .fold(Term::nil(), |acc, val| cons!(heap, val, acc));
    Ok(res)
}

use std::cmp;

/// Longest Common Prefix
///
/// Given a vector of string slices, calculate the string
/// slice that is the longest common prefix of the strings.
///
/// ```
/// let words = vec!["zebrawood", "zebrafish", "zebra mussel"];
/// let prefix = longest_common_prefix(words);
/// assert_eq!(prefix, "zebra");
/// ```
pub fn longest_common_prefix(strings: &[Vec<u8>]) -> usize {
    if strings.is_empty() {
        return 0;
    }
    let str0 = &strings[0];
    let mut len = str0.len();
    for str in &strings[1..] {
        len = cmp::min(
            len,
            str.iter().zip(str0).take_while(|&(a, b)| a == b).count(),
        );
    }
    len
}

pub fn longest_common_prefix_1(
    _vm: &vm::Machine,
    process: &RcProcess,
    args: &[Term],
) -> bif::Result {
    let heap = &process.context_mut().heap;
    let mut iter = args[0];
    let mut acc = Vec::new();
    while let Ok(Cons { head, tail }) = Cons::cast_from(&iter) {
        // TODO: error handling
        let bytes = head.to_bytes().unwrap();
        acc.push(bytes.to_vec()); // TODO: this is not great since we're looping and can't use a ref
        iter = *tail;
    }

    if !iter.is_nil() {
        return Err(badarg!());
    }

    Ok(Term::uint64(heap, longest_common_prefix(&acc) as u64))
}

fn copy(bytes: &[u8], n: usize) -> Binary {
    let new_size = bytes.len() * n;
    let mut buf = Vec::with_capacity(new_size);
    for _ in 0..n {
        buf.extend_from_slice(bytes);
    }

    Binary::from(buf)
}

pub fn copy_1(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    let heap = &process.context_mut().heap;
    let bytes = match args[0].to_bytes() {
        Some(bytes) => bytes,
        _ => return Err(badarg!()),
    };

    let bin = copy(bytes, 1);
    Ok(Term::binary(heap, bin))
}

pub fn copy_2(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    let heap = &process.context_mut().heap;
    let bytes = match args[0].to_bytes() {
        Some(bytes) => bytes,
        _ => return Err(badarg!()),
    };

    let n = match args[1].into_variant() {
        Variant::Integer(i) if i >= 0 => i as usize,
        _ => return Err(badarg!()),
    };

    let bin = copy(bytes, n);
    Ok(Term::binary(heap, bin))
}

pub fn first_1(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    let heap = &process.context_mut().heap;
    let bytes = match args[0].to_bytes() {
        Some(bytes) => bytes,
        _ => return Err(badarg!()),
    };

    match bytes.first() {
        Some(b) => Ok(Term::uint(heap, u32::from(*b))),
        None => Err(badarg!()),
    }
}

pub fn last_1(_vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> bif::Result {
    let heap = &process.context_mut().heap;
    let bytes = match args[0].to_bytes() {
        Some(bytes) => bytes,
        _ => return Err(badarg!()),
    };

    match bytes.last() {
        Some(b) => Ok(Term::uint(heap, u32::from(*b))),
        None => Err(badarg!()),
    }
}
