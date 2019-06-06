//use crate::servo_arc::Arc;
use regex::bytes::Regex;

// pub mod error;
// use std::error::Error;
// pub use error::Result;

pub mod bif {
    use super::*;
    use crate::bif::Result;
    use crate::exception::{self, Exception, Reason};
    use crate::process::RcProcess;
    use crate::value::{self, Term};
    use crate::vm;

    pub fn run_3(vm: &vm::Machine, process: &RcProcess, args: &[Term]) -> Result {
        let heap = &process.context_mut().heap;
        let string = crate::bif::erlang::list_to_iodata(args[0]).unwrap(); // TODO: error handling
        let pattern = crate::bif::erlang::list_to_iodata(args[1]).unwrap(); // TODO: error handling

        //println!("inspect {:?} -- {:?}", string, pattern);
        // str = [115, 117, 112, 101, 114, 118, 105, 115, 111, 114, 58, 32, 123, 108, 111, 99, 97, 108, 44, 107, 101, 114, 110, 101, 108, 95, 115, 117, 112, 125, 10, 32, 32, 32, 32, 115, 116, 97, 114, 116, 101, 100, 58, 32, 91, 123, 112, 105, 100, 44, 60, 48, 46, 48, 46, 52, 52, 62, 125, 44, 123, 105, 100, 44, 99, 111, 100, 101, 95, 115, 101, 114, 118, 101, 114, 125, 44, 123, 109, 102, 97, 114, 103, 115, 44, 123, 99, 111, 100, 101, 44, 115, 116, 97, 114, 116, 95, 108, 105, 110, 107, 44, 91, 93, 125, 125, 44, 123, 114, 101, 115, 116, 97, 114, 116, 95, 116, 121, 112, 101, 44, 112, 101, 114, 109, 97, 110, 101, 110, 116, 125, 44, 123, 115, 104, 117, 116, 100, 111, 119, 110, 44, 50, 48, 48, 48, 125, 44, 123, 99, 104, 105, 108, 100, 95, 116, 121, 112, 101, 44, 119, 111, 114, 107, 101, 114, 125, 93]
        // pat = [44, 63, 13, 63, 10, 32, 42]
        // iex(5)> :re.run(str, pat, [:unicode, :global])
        // {:match, [[{30, 5}]]}

        // TODO verify args
        let regex = Regex::new(std::str::from_utf8(&pattern).unwrap()).unwrap();

        let res = regex.find_iter(&string).fold(Term::nil(), |acc, m| {
            cons!(
                heap,
                tup2!(
                    heap,
                    Term::uint64(heap, m.start() as u64),
                    Term::uint64(heap, (m.end() - m.start()) as u64)
                ),
                acc
            )
        });

        // println!("re_run_3 inputs {:?} and {:?}", string, pattern);
        println!("re_run_3 res {}", res);

        if !res.is_nil() {
            Ok(tup2!(heap, atom!(MATCH), cons!(heap, res, Term::nil())))
        } else {
            Ok(atom!(NOMATCH))
        }
    }
}
