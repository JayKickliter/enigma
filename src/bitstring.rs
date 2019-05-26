use crate::immix::Heap;
use crate::process::RcProcess;
use crate::servo_arc::Arc;
use crate::value::{self, Term, TryFrom};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
// use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
// use std::cell::UnsafeCell;

/// make_mask(n) constructs a mask with n bits.
/// Example: make_mask!(3) returns the binary number 00000111.
macro_rules! make_mask {
    ($n:expr) => {
        // we use u16 then go down to u8 because (1 << 8) overflows
        (((1 as u16) << $n) - 1) as u8
    };
}

/// mask_bits assigns src to dst, but preserves the dst bits outside the mask.
macro_rules! mask_bits {
    ($src:expr, $dst:expr, $mask:expr) => {
        ($src & $mask) | ($dst & !$mask)
    };
}

/// nbytes!(x) returns the number of bytes needed to store x bits.
macro_rules! nbytes {
    ($x:expr) => {
        (($x as u64 + 7) >> 3) as usize
    };
}

macro_rules! byte_offset {
    ($ofs:expr) => {
        $ofs as usize >> 3
    };
}

macro_rules! bit_offset {
    ($ofs:expr) => {
        $ofs & 7
    };
}

// TODO: replace RcBinary by a binary that keeps Bytes/BytesMut

#[derive(Debug)]
pub struct Binary {
    // pub flags: AtomicUsize, // TODO use AtomicU8 once integer_atomics lands in rust 1.33
    pub is_writable: bool,

    /// The actual underlying bits.
    pub data: Vec<u8>,
}

pub type RcBinary = Arc<Binary>;

impl Binary {
    pub fn new() -> Self {
        Binary {
            // flags: AtomicUsize::new(0),
            // WRITABLE | ACTIVE_WRITER
            is_writable: true,
            data: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Binary {
            // flags: AtomicUsize::new(0),
            // WRITABLE | ACTIVE_WRITER
            is_writable: true,
            data: Vec::with_capacity(cap),
        }
    }

    pub fn with_size(size: usize) -> Self {
        Binary {
            // flags: AtomicUsize::new(0),
            // WRITABLE | ACTIVE_WRITER
            is_writable: true,
            data: vec![0; size],
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self) -> &mut Vec<u8> {
        // :( we want to avoid locks so this method is for specifically when we know we're the only writer.
        unsafe { &mut *(&self.data as *const Vec<u8> as *mut Vec<u8>) }
    }
}

impl From<Vec<u8>> for Binary {
    fn from(value: Vec<u8>) -> Self {
        Binary {
            // flags: AtomicUsize::new(0),
            // WRITABLE | ACTIVE_WRITER
            is_writable: true,
            data: value,
        }
    }
}

impl From<&[u8]> for Binary {
    fn from(value: &[u8]) -> Self {
        Binary {
            // flags: AtomicUsize::new(0),
            // WRITABLE | ACTIVE_WRITER
            is_writable: true,
            data: value.to_vec(),
        }
    }
}

impl AsRef<[u8]> for Binary {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl AsRef<[u8]> for Arc<Binary> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl Ord for Binary {
    fn cmp(&self, other: &Binary) -> Ordering {
        self.data.cmp(&other.data)
    }
}

impl PartialOrd for Binary {
    fn partial_cmp(&self, other: &Binary) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Binary {
    fn eq(&self, other: &Binary) -> bool {
        self.data == other.data
    }
}

impl Eq for Binary {}

impl Hash for Binary {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state)
    }
}

// TODO: to be TryFrom once rust stabilizes the trait
impl TryFrom<Term> for RcBinary {
    type Error = value::WrongBoxError;

    #[inline]
    fn try_from(value: &Term) -> Result<&Self, value::WrongBoxError> {
        if let value::Variant::Pointer(ptr) = value.into_variant() {
            unsafe {
                if *ptr == value::BOXED_BINARY {
                    return Ok(&(*(ptr as *const value::Boxed<Self>)).value);
                }
            }
        }
        Err(value::WrongBoxError)
    }
}

/// Binaries are bitstrings by default, byte aligned ones are binaries.
#[derive(Clone, Debug)]
pub struct SubBinary {
    // TODO: wrap into value
    /// Binary size in bytes
    pub size: usize,
    /// Offset into binary in bytes
    pub offset: usize,
    /// Bit size
    pub bitsize: usize,
    /// Bit offset
    pub bit_offset: u8,
    /// Is the underlying binary writable?
    pub is_writable: bool,
    /// Original binary (refc or heap)
    pub original: RcBinary,
} // TODO: I don't like pub here, have a method (binary_data()) or something

// TODO: to be TryFrom once rust stabilizes the trait
impl TryFrom<Term> for SubBinary {
    type Error = value::WrongBoxError;

    #[inline]
    fn try_from(value: &Term) -> Result<&Self, value::WrongBoxError> {
        if let value::Variant::Pointer(ptr) = value.into_variant() {
            unsafe {
                if *ptr == value::BOXED_SUBBINARY {
                    return Ok(&(*(ptr as *const value::Boxed<Self>)).value);
                }
            }
        }
        Err(value::WrongBoxError)
    }
}

impl SubBinary {
    pub fn new(original: RcBinary, num_bits: usize, offset: usize, is_writable: bool) -> Self {
        SubBinary {
            original,
            size: byte_offset!(num_bits),
            bitsize: bit_offset!(num_bits),
            offset: byte_offset!(offset),
            bit_offset: bit_offset!(offset as u8), // TODO looks wrong
            is_writable,
        }
    }

    pub fn is_binary(&self) -> bool {
        self.bitsize & 7 == 0
    }
}

// TODO: let's use nom to handle offsets & matches, and keep a reference to the binary
// TODO: implement io::Read/io::Write
#[derive(Debug)]
pub struct MatchBuffer {
    /// Original binary
    pub original: RcBinary,
    /// Current position in binary
    // base: usize, // TODO: actually a ptr?
    /// Offset in bits
    pub offset: usize, // TODO: maybe don't make these pub, add a remainder method
    /// Size of binary in bits
    pub size: usize,
}

impl From<RcBinary> for MatchBuffer {
    fn from(original: RcBinary) -> Self {
        let len = original.data.len();
        MatchBuffer {
            original,
            //base: binary_bytes(original),
            offset: 0,
            size: len * 8,
        }
    }
}

impl From<SubBinary> for MatchBuffer {
    fn from(binary: SubBinary) -> Self {
        // let len = binary.original.data.len();
        let len = binary.size;
        let offset = 8 * binary.offset + binary.bit_offset as usize;

        MatchBuffer {
            original: binary.original,
            //base: binary_bytes(original),
            offset,
            size: len * 8 + offset + binary.bitsize, // + offset so that we get correct remaining()
        }
    }
}

pub struct MatchState {
    // TODO: wrap into value
    pub mb: MatchBuffer,
    /// Saved offsets, only valid for contexts created through bs_start_match2.
    pub saved_offsets: Vec<usize>,
} // TODO: Dump start_match_2 support. use MatchBuffer directly

// TODO: to be TryFrom once rust stabilizes the trait
impl TryFrom<Term> for MatchState {
    type Error = value::WrongBoxError;

    #[inline]
    fn try_from(value: &Term) -> Result<&Self, value::WrongBoxError> {
        if let value::Variant::Pointer(ptr) = value.into_variant() {
            unsafe {
                if *ptr == value::BOXED_MATCHSTATE {
                    return Ok(&(*(ptr as *const value::Boxed<Self>)).value);
                }
            }
        }
        Err(value::WrongBoxError)
    }
}

bitflags! {
    /// Flags for bs_get_* / bs_put_* / bs_init* instructions.
    pub struct Flag: u8 {
        const BSF_NONE = 0;
        /// Field is guaranteed to be byte-aligned. TODO: seems unused?
        const BSF_ALIGNED = 1;
        /// Field is little-endian (otherwise big-endian).
        const BSF_LITTLE = 2;
        /// Field is signed (otherwise unsigned).
        const BSF_SIGNED = 4;
        /// Size in bs_init is exact. TODO: seems unused?
        const BSF_EXACT = 8;
        /// Native endian.
        const BSF_NATIVE = 16;
    }
}

#[cfg(target_endian = "little")]
const NATIVE_ENDIAN: Flag = Flag::BSF_LITTLE;

#[cfg(target_endian = "big")]
const NATIVE_ENDIAN: Flag = Flag::BSF_NONE;

macro_rules! bit_is_machine_endian {
    ($x:expr) => {
        $x & Flag::BSF_LITTLE == NATIVE_ENDIAN
    };
}

#[cfg(target_endian = "little")]
macro_rules! native_endian {
    ($x:expr) => {
        if $x.contains(Flag::BSF_NATIVE) {
            $x.remove(Flag::BSF_NATIVE);
            $x.insert(Flag::BSF_LITTLE);
        }
    };
}

#[cfg(target_endian = "big")]
macro_rules! native_endian {
    ($x:expr) => {
        if $x.contains(Flag::BSF_NATIVE) {
            $x.remove(Flag::BSF_NATIVE);
            $x.remove(Flag::BSF_LITTLE);
        }
    };
}

macro_rules! binary_size {
    ($str:expr) => {
        // TODO: use a trait
        match $str.get_boxed_header() {
            Ok(value::BOXED_BINARY) => $str.get_boxed_value::<RcBinary>().unwrap().data.len(),
            Ok(value::BOXED_SUBBINARY) => {
                // TODO use ok_or to cast to some, then use ?
                let sub = $str.get_boxed_value::<SubBinary>().unwrap();
                let mut size = sub.size;

                if sub.bitsize > 0 {
                    // round up
                    size += 1;
                }
                size
            }
            _ => unreachable!(),
        }
    };
}

// pub fn start_match_2(heap: &Heap, binary: Term, _max: u32) -> Option<Term> {
//     assert!(binary.is_bitstring());

//     // TODO: BEAM allocates size on all binary types right after the header so we can grab it
//     // without needing the binary subtype.
//     let total_bin_size = binary_size!(binary);

//     if (total_bin_size >> (8 * std::mem::size_of::<usize>() - 3)) != 0 {
//         // Uint => maybe u8??
//         return None;
//     }

//     // TODO: this is not nice
//     let mb = match binary.get_boxed_header() {
//         Ok(value::BOXED_BINARY) => {
//             // TODO use ok_or to cast to some, then use ?
//             let value = binary.get_boxed_value::<RcBinary>().unwrap().clone();
//             MatchBuffer::from(value)
//         }
//         Ok(value::BOXED_SUBBINARY) => {
//             // TODO use ok_or to cast to some, then use ?
//             let value = binary.get_boxed_value::<SubBinary>().unwrap().clone();
//             MatchBuffer::from(value)
//         }
//         _ => unreachable!(),
//     };

//     // TODO: toggle is_writable to false for rcbinary!
//     // pb = (ProcBin *) boxed_val(Orig);
//     // if (pb->thing_word == HEADER_PROC_BIN && pb->flags != 0) {
//     //  erts_emasculate_writable_binary(pb);
//     // }

//     Some(Term::matchstate(
//         heap,
//         MatchState {
//             saved_offsets: vec![mb.offset],
//             mb,
//         },
//     ))
// }

pub fn start_match_3(heap: &Heap, binary: Term) -> Option<Term> {
    assert!(binary.is_bitstring());

    // TODO: BEAM allocates size on all binary types right after the header so we can grab it
    // without needing the binary subtype.
    let total_bin_size = binary_size!(binary);

    if (total_bin_size >> (8 * std::mem::size_of::<usize>() - 3)) != 0 {
        // Uint => maybe u8??
        return None;
    }

    // TODO: this is not nice
    let mb = match binary.get_boxed_header() {
        Ok(value::BOXED_BINARY) => {
            // TODO use ok_or to cast to some, then use ?
            let value = binary.get_boxed_value::<RcBinary>().unwrap().clone();
            MatchBuffer::from(value)
        }
        Ok(value::BOXED_SUBBINARY) => {
            // TODO use ok_or to cast to some, then use ?
            let value = binary.get_boxed_value::<SubBinary>().unwrap().clone();
            MatchBuffer::from(value)
        }
        _ => unreachable!(),
    };

    // TODO: toggle is_writable to false for rcbinary!
    // pb = (ProcBin *) boxed_val(Orig);
    // if (pb->thing_word == HEADER_PROC_BIN && pb->flags != 0) {
    //  erts_emasculate_writable_binary(pb);
    // }

    Some(Term::matchstate(
        heap,
        MatchState {
            saved_offsets: vec![],
            mb,
        },
    ))
}

// #ifdef DEBUG
// # define CHECK_MATCH_BUFFER(MB) check_match_buffer(MB)

// static void check_match_buffer(ErlBinMatchBuffer* mb)
// {
//     Eterm realbin;
//     Uint byteoffs;
//     byte* bytes, bitoffs, bitsz;
//     ProcBin* pb;
//     ERTS_GET_REAL_BIN(mb->orig, realbin, byteoffs, bitoffs, bitsz);
//     bytes = binary_bytes(realbin) + byteoffs;
//     ERTS_ASSERT(mb->base >= bytes && mb->base <= (bytes + binary_size(mb->orig)));
//     pb = (ProcBin *) boxed_val(realbin);
//     if (pb->thing_word == HEADER_PROC_BIN)
//         ERTS_ASSERT(pb->flags == 0);
// }
// #else
// # define CHECK_MATCH_BUFFER(MB)
// #endif

const SMALL_BITS: usize = 64;

impl MatchBuffer {
    #[inline(always)]
    pub fn remaining(&self) -> usize {
        self.size - self.offset
    }

    /// This function returns a Cow so we do zero-copy fetching if it's aligned.
    pub fn get_bytes(&mut self, num_bytes: usize) -> Option<std::borrow::Cow<[u8]>> {
        let num_bits = num_bytes * 8;
        if self.remaining() < num_bits {
            // Asked for too many bits.
            return None;
        }

        let byte_offset = byte_offset!(self.offset);

        if self.offset % 8 == 0 {
            // aligned, advance cursor and return as a borrowed
            self.offset += num_bits;
            return Some(std::borrow::Cow::Borrowed(
                &self.original.data[byte_offset..byte_offset + num_bytes],
            ));
        }
        // unaligned, copy and return as owned
        let mut buf = vec![0; num_bytes];
        unsafe {
            copy_bits(
                self.original.data.as_ptr(),
                byte_offset,
                1,
                buf.as_mut_ptr(),
                0,
                1,
                num_bits,
            )
        };
        self.offset += num_bits;
        Some(std::borrow::Cow::Owned(buf))
    }

    pub fn get_integer(&mut self, heap: &Heap, num_bits: usize, mut flags: Flag) -> Option<Term> {
        //    Uint bytes;
        //    Uint bits;
        //    Uint offs;
        //    byte bigbuf[64];
        //    byte* LSB;
        //    byte* MSB;
        //    Uint* hp;
        //    Uint words_needed;
        //    Uint actual;
        //    Uint v32;
        //    int sgn = 0;
        //    Eterm res = THE_NON_VALUE;

        // TODO: preprocess flags for native endian in loader(remove native_endian and set bsf_little off or on)
        native_endian!(flags);

        if num_bits == 0 {
            return Some(Term::from(0));
        }

        // CHECK_MATCH_BUFFER(mb);

        if self.remaining() < num_bits {
            // Asked for too many bits.
            return None;
        }

        // Special cases for field sizes up to the size of Uint.

        let offs = bit_offset!(self.offset);

        if num_bits <= 8 - offs {
            // All bits are in one byte in the binary. We only need shift them right and mask them.

            let mut b: u8 = self.original.data[byte_offset!(self.offset)];
            let mask = make_mask!(num_bits);
            self.offset += num_bits;
            b >>= 8 - offs - num_bits;
            b &= mask;
            // need to transmute to signed (i8)
            // if ((flags & BSF_SIGNED) && b >> (num_bits-1)) {
            //     b |= ~mask;
            // }
            return Some(Term::int(i32::from(b)));
        } else if num_bits <= 8 {
            /*
             * The bits are in two different bytes. It is easiest to
             * combine the bytes to a word first, and then shift right and
             * mask to extract the bits.
             */
            let byte_offset = byte_offset!(self.offset);
            let mut w: u16 = (self.original.data[byte_offset] as u16) << 8
                | (self.original.data[byte_offset + 1] as u16);
            let mask = make_mask!(num_bits) as u16;
            self.offset += num_bits;
            w >>= 16 - offs - num_bits;
            w &= mask;
            // if ((flags & BSF_SIGNED) && w >> (num_bits-1)) {
            //     w |= ~mask;
            // }
            return Some(Term::int(i32::from(w)));
        } else if num_bits < SMALL_BITS && !flags.contains(Flag::BSF_LITTLE) {
            /*
             * Handle field sizes from 9 up to SMALL_BITS-1 bits, big-endian,
             * stored in at least two bytes.
             */
            let mut byte_offset = byte_offset!(self.offset);

            let mut n = num_bits;
            self.offset += num_bits;

            /*
             * Handle the most signicant byte if it contains 1 to 7 bits.
             * It only needs to be masked, not shifted.
             */
            let mut w: u32;
            if offs == 0 {
                w = 0;
            } else {
                let num_bits_in_msb = 8 - offs;
                w = self.original.data[byte_offset] as u32;
                byte_offset += 1;
                n -= num_bits_in_msb;
                w &= make_mask!(num_bits_in_msb) as u32;
            }

            // Simply shift whole bytes into the result.
            for _ in 0..byte_offset!(n) {
                w = (w << 8) | (self.original.data[byte_offset] as u32);
                byte_offset += 1;
            }
            n = bit_offset!(n);

            /*
             * Handle the 1 to 7 bits remaining in the last byte (if any).
             * They need to be shifted right, but there is no need to mask;
             * then they can be shifted into the word.
             */
            if n > 0 {
                let mut b: u8 = self.original.data[byte_offset];
                b >>= 8 - n;
                w = (w << n) | (b as u32);
            }

            /*
             * Sign extend the result if the field type is 'signed' and the
             * most significant bit is 1.
             */
            //   if ((flags & BSF_SIGNED) != 0 && (w >> (num_bits-1) != 0)) {
            //       w |= ~MAKE_MASK(num_bits);
            //   }
            return Some(Term::uint(heap, w));
        }
        // TODO: this is not nice

        /*
         * Handle everything else, that is:
         *
         * Big-endian fields >= SMALL_BITS (potentially bignums).
         * Little-endian fields with 9 or more bits.
         */

        // bytes = NBYTES(num_bits);
        // if ((bits = BIT_OFFSET(num_bits)) == 0) {  /* number of bits in MSB */
        //   bits = 8;
        // }
        // offs = 8 - bits;                  /* adjusted offset in MSB */
        //
        // if (bytes <= sizeof bigbuf) {
        //   LSB = bigbuf;
        // } else {
        //   LSB = erts_alloc(ERTS_ALC_T_TMP, bytes);
        // }
        // MSB = LSB + bytes - 1;

        /*
         * Move bits to temporary buffer. We want the buffer to be stored in
         * little-endian order, since bignums are little-endian.
         */

        // if (flags & BSF_LITTLE) {
        //   erts_copy_bits(mb->base, mb->offset, 1, LSB, 0, 1, num_bits);
        //   *MSB >>= offs;		/* adjust msb */
        // } else {
        //   *MSB = 0;
        //   erts_copy_bits(mb->base, mb->offset, 1, MSB, offs, -1, num_bits);
        // }
        // mb->offset += num_bits;

        /*
         * Get the sign bit.
         */
        // sgn = 0;
        // if ((flags & BSF_SIGNED) && (*MSB & (1<<(bits-1)))) {
        //   byte* ptr = LSB;
        //   byte c = 1;
        //
        //   /* sign extend MSB */
        //   *MSB |= ~MAKE_MASK(bits);
        //
        //   /* two's complement */
        //   while (ptr <= MSB) {
        //       byte pd = ~(*ptr);
        //       byte d = pd + c;
        //       c = (d < pd);
        //       *ptr++ = d;
        //   }
        //   sgn = 1;
        // }

        /* normalize */
        // while ((*MSB == 0) && (MSB > LSB)) {
        //   MSB--;
        //   bytes--;
        // }

        /* check for guaranteed small num */
        // switch (bytes) {
        // case 1:
        //   v32 = LSB[0];
        //   goto big_small;
        // case 2:
        //   v32 = LSB[0] + (LSB[1]<<8);
        //   goto big_small;
        // case 3:
        //   v32 = LSB[0] + (LSB[1]<<8) + (LSB[2]<<16);
        //   goto big_small;
        //#if !defined(ARCH_64)
        // case 4:
        //   v32 = (LSB[0] + (LSB[1]<<8) + (LSB[2]<<16) + (LSB[3]<<24));
        //   if (!IS_USMALL(sgn, v32)) {
        //      goto make_big;
        //   }
        //#else
        // case 4:
        //   ReadToVariable(v32, LSB, 4);
        //   goto big_small;
        // case 5:
        //   ReadToVariable(v32, LSB, 5);
        //   goto big_small;
        // case 6:
        //   ReadToVariable(v32, LSB, 6);
        //   goto big_small;
        // case 7:
        //   ReadToVariable(v32, LSB, 7);
        //   goto big_small;
        // case 8:
        //   ReadToVariable(v32, LSB, 8);
        //   if (!IS_USMALL(sgn, v32)) {
        //   goto make_big;
        // }
        //#endif
        // big_small:			/* v32 loaded with value which fits in fixnum */
        //   if (sgn) {
        //       res = make_small(-((Sint)v32));
        //   } else {
        //       res = make_small(v32);
        //   }
        //   break;
        // make_big:
        //   hp = HeapOnlyAlloc(p, BIG_UINT_HEAP_SIZE);
        //   if (sgn) {
        //     hp[0] = make_neg_bignum_header(1);
        //   } else {
        //     hp[0] = make_pos_bignum_header(1);
        //   }
        //   BIG_DIGIT(hp,0) = v32;
        //   res = make_big(hp);
        //   break;
        // default:
        //   words_needed = 1+WSIZE(bytes);
        //   hp = HeapOnlyAlloc(p, words_needed);
        //   res = bytes_to_big(LSB, bytes, sgn, hp);
        //   if (is_nil(res)) {
        //       p->htop = hp;
        //       res = THE_NON_VALUE;
        //   } else if (is_small(res)) {
        //       p->htop = hp;
        //   } else if ((actual = bignum_header_arity(*hp)+1) < words_needed) {
        //       p->htop = hp + actual;
        //   }
        //   break;
        // }
        //
        // if (LSB != bigbuf) {
        //   erts_free(ERTS_ALC_T_TMP, (void *) LSB);
        // }
        // return res;
        unimplemented!(
            "get_integer {:?}, num_bits {}, flags: {:?}",
            self,
            num_bits,
            flags
        );
        // get_integer MatchBuffer { original: Binary { is_writable: true, data: [204, 0, 0, 0, 63, 0, 0, 0] }, offset: 0, size: 64 }, num_bits 32, flags: BSF_LITTLE'
        // <<W:32/native,H:32/native>> = list_to_binary(List),
    }

    pub fn get_float(&mut self, _heap: &Heap, num_bits: usize, mut flags: Flag) -> Option<Term> {
        let mut fl32: f32 = 0.0;
        let mut fl64: f64 = 0.0;

        // TODO: preprocess flags for native endian in loader(remove native_endian and set bsf_little off or on)
        native_endian!(flags);

        // CHECK_MATCH_BUFFER(mb);

        if num_bits == 0 {
            return Some(Term::from(0.0));
        }

        if self.remaining() < num_bits {
            // Asked for too many bits.
            return None;
        }

        let fptr: *mut u8 = match num_bits {
            32 => &mut fl32 as *mut f32 as *mut u8,
            64 => &mut fl64 as *mut f64 as *mut u8,
            _ => return None,
        };

        if bit_is_machine_endian!(flags) {
            unsafe {
                copy_bits(
                    self.original.data.as_ptr(),
                    self.offset,
                    1,
                    fptr,
                    0,
                    1,
                    num_bits,
                )
            };
        } else {
            unsafe {
                copy_bits(
                    self.original.data.as_ptr(),
                    self.offset,
                    1,
                    fptr.add(nbytes!(num_bits) - 1),
                    0,
                    -1,
                    num_bits,
                )
            };
        }

        let f = if num_bits == 32 {
            f64::from(fl32)
        } else {
            //   #ifdef DOUBLE_MIDDLE_ENDIAN
            //   FloatDef ftmp;
            //   ftmp.fd = f64;
            //   f.fw[0] = ftmp.fw[1];
            //   f.fw[1] = ftmp.fw[0];
            //   ERTS_FP_ERROR_THOROUGH(p, f.fd, return THE_NON_VALUE);
            //   #else
            //   ...
            //   #endif
            fl64
        };
        if !f.is_finite() {
            return None;
        }
        self.offset += num_bits;
        Some(Term::from(f))
    }

    pub fn get_binary(&mut self, heap: &Heap, num_bits: usize, _flags: Flag) -> Option<Term> {
        // CHECK_MATCH_BUFFER(mb);

        // Reduce the use of none by using Result.
        if self.remaining() < num_bits {
            // Asked for too many bits.
            return None;
        }

        // From now on, we can't fail.

        let binary = Term::subbinary(
            heap,
            SubBinary::new(self.original.clone(), num_bits, self.offset, false),
        );
        self.offset += num_bits;
        Some(binary)
    }

    pub fn get_binary_all(&mut self, heap: &Heap, _flags: Flag) -> Option<Term> {
        // CHECK_MATCH_BUFFER(mb);

        let size = self.remaining();
        let binary = Term::subbinary(
            heap,
            SubBinary::new(self.original.clone(), size, self.offset, false),
        );
        self.offset = size;
        Some(binary)
    }

    /// Copy up to 4 bytes into the supplied buffer.
    #[inline]
    fn align_utf8_bytes(&self, buf: *mut u8) {
        let bits = match self.remaining() {
            0...7 => unreachable!(),
            8...15 => 8,
            16...23 => 24,
            24...31 => 24,
            _ => 32,
        };

        unsafe { copy_bits(self.original.data.as_ptr(), self.offset, 1, buf, 0, 1, bits) }
    }

    pub fn get_utf8(&mut self) -> Option<Term> {
        // Number of trailing bytes for each value of the first byte.
        const TRAILING_BYTES_FOR_UTF8: [u8; 256] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
            9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
            9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2,
            2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 9, 9, 9, 9, 9, 9, 9, 9,
        ];

        // CHECK_MATCH_BUFFER(mb);
        let remaining_bits = self.remaining();
        if remaining_bits < 8 {
            return None;
        }

        let mut tmp_buf: [u8; 4] = [0; 4];

        let pos: &[u8] = if bit_offset!(self.offset) == 0 {
            let offset = byte_offset!(self.offset);
            &self.original.data[offset..]
        } else {
            self.align_utf8_bytes(tmp_buf.as_mut_ptr());
            &tmp_buf[..]
        };

        let result = pos[0] as usize;
        let result = match TRAILING_BYTES_FOR_UTF8[result] {
            0 => {
                // One byte only
                self.offset += 8;
                result
            }
            1 => {
                // Two bytes
                if remaining_bits < 16 {
                    return None;
                }
                let a = pos[1] as usize;
                if (a & 0xC0) != 0x80 {
                    return None;
                }
                let result = (result << 6) + a - 0x0000_3080;
                self.offset += 16;
                result
            }
            2 => {
                // Three bytes
                if remaining_bits < 24 {
                    return None;
                }
                let a = pos[1] as usize;
                let b = pos[2] as usize;
                if (a & 0xC0) != 0x80 || (b & 0xC0) != 0x80 || (result == 0xE0 && a < 0xA0) {
                    return None;
                }
                let result = (((result << 6) + a) << 6) + b - 0x000E_2080;
                if 0xD800 <= result && result <= 0xDFFF {
                    return None;
                }
                self.offset += 24;
                result
            }
            3 => {
                // Four bytes
                if remaining_bits < 32 {
                    return None;
                }
                let a = pos[1] as usize;
                let b = pos[2] as usize;
                let c = pos[3] as usize;
                if (a & 0xC0) != 0x80
                    || (b & 0xC0) != 0x80
                    || (c & 0xC0) != 0x80
                    || (result == 0xF0 && a < 0x90)
                {
                    return None;
                }
                let result = (((((result << 6) + a) << 6) + b) << 6) + c - 0x03C8_2080;
                if result > 0x0010_FFFF {
                    return None;
                }
                self.offset += 32;
                result
            }
            _ => unreachable!(),
        };

        Some(Term::int(result as i32)) // potentionally unsafe?
    }

    pub fn get_utf16(&mut self, flags: Flag) -> Option<Term> {
        let remaining_bits = self.remaining();
        if remaining_bits < 16 {
            return None;
        }

        let mut tmp_buf: [u8; 4] = [0; 4];

        // CHECK_MATCH_BUFFER(mb);
        // Set up the pointer to the source bytes.
        let src: &[u8] = if bit_offset!(self.offset) == 0 {
            /* We can access the binary directly because the bytes are aligned. */
            let offset = byte_offset!(self.offset);
            &self.original.data[offset..]
        } else {
            /*
             * We must copy the data to a temporary buffer. If possible,
             * get 4 bytes, otherwise two bytes.
             */
            let n = if remaining_bits < 32 { 16 } else { 32 };
            unsafe {
                copy_bits(
                    self.original.data.as_ptr(),
                    self.offset,
                    1,
                    tmp_buf.as_mut_ptr(),
                    0,
                    1,
                    n,
                )
            };
            &tmp_buf[..]
        };

        // Get the first (and maybe only) 16-bit word. See if we are done.
        let w1: u16 = if flags.contains(Flag::BSF_LITTLE) {
            u16::from(src[0]) | (u16::from(src[1]) << 8)
        } else {
            (u16::from(src[0]) << 8) | u16::from(src[1])
        };
        if w1 < 0xD800 || w1 > 0xDFFF {
            self.offset += 16;
            return Some(Term::int(i32::from(w1)));
        } else if w1 > 0xDBFF {
            return None;
        }

        // Get the second 16-bit word and combine it with the first.
        let w2: u16 = if remaining_bits < 32 {
            return None;
        } else if flags.contains(Flag::BSF_LITTLE) {
            u16::from(src[2]) | (u16::from(src[3]) << 8)
        } else {
            (u16::from(src[2]) << 8) | u16::from(src[3])
        };
        if !(0xDC00 <= w2 && w2 <= 0xDFFF) {
            return None;
        }
        self.offset += 32;
        Some(Term::int(
            ((((w1 as u32 & 0x3FF) << 10) | (w2 as u32 & 0x3FF)) + 0x10000) as i32, // potentially unsafe
        ))
    }
}

// Stores data on the process heap. Small, but expensive to copy.
// HeapBin(len + ptr)
// Stores data off the process heap, in an Arc<>. Cheap to copy around.
// RefBin(Arc<String/Vec<u8?>>)
// ^^ start with just RefBin since Rust already will do the String management for us
// SubBin(len (original?), offset, bitsize,bitoffset,is_writable, orig_ptr -> Bin/RefBin)

// consider using an Arc<RwLock<>> to make the inner string mutable? is the overhead worth it?
// data is always append only, so maybe have an atomic bool for the writable bit and keep the
// normal structure lockless.

macro_rules! copy_binary {
    ($dst:expr, $dst_offset:expr, $src:expr, $src_offset:expr, $num_bits:expr) => {
        // TODO: isn't this already implemented inside copy_bits??
        unsafe {
            if bit_offset!($dst_offset) == 0
                && ($src_offset == 0)
                && (bit_offset!($num_bits) == 0)
                && ($num_bits != 0)
            {
                let byte_size = nbytes!($num_bits);
                let src_slice = std::slice::from_raw_parts($src, byte_size);
                let dst_slice =
                    std::slice::from_raw_parts_mut($dst.add(byte_offset!($dst_offset)), byte_size);
                dst_slice.copy_from_slice(src_slice);
            } else {
                copy_bits($src, $src_offset, 1, $dst, $dst_offset, 1, $num_bits);
            }
        }
    };
}

// check the binary
//if !boxed (sub)binary type return badarg
// if !type subbin or !subbin.is_writable or not subbin.origin.flags is_witable
// --> binary not writable (create new binary and copy old contents)
//   --> calculate size
//   --> check if size hits system limits
//   --> something about bin_size with smallest size being 256
//   --> allocate bin on the heap (writable & active_writer on)
//   --> allocate subbin pointing to heap
//   --> repoint current_bin to the bin
//   --> copy old binary data to the new one
//   --> return nrew subbin
//
// --> binary writable (reuse bin, make new subbin)
//   --> check unit size
//   --> check if build size fits onto htop, else garbage collect
//   --> check we didn't hit system limits
//   --> mark subbin as is_writable false
//   --> mark bin as active writer
//   --> resize binary if too small
//   --> build a new subbinary pointing to bin with writable = 1
//   --> repoint current_bin to the bin
//   --> return the new subbin
//

pub fn append(
    process: &RcProcess,
    binary: Term,
    build_size: Term,
    _extra_words: usize,
    unit: usize,
) -> Option<Term> {
    // Check and untag the requested build size.
    // if size < 0 || not smallint
    let build_size_in_bits = match build_size.into_variant() {
        value::Variant::Integer(i) if i >= 0 => i,
        // TODO: return err reason probs instead of tweaking freason
        // _ => return Err(Exception::new(Reason::EXC_BADARG)),
        _ => return None,
        // p->freason = BADARG;
    };

    let heap = &process.context_mut().heap;

    // Check the binary argument.

    // TODO: this is not nice
    let writable = match binary.get_boxed_header() {
        Ok(value::BOXED_SUBBINARY) => {
            // TODO use ok_or to cast to some, then use ?
            let sb = &binary.get_boxed_value::<SubBinary>().unwrap();

            sb.is_writable && sb.original.is_writable
        }
        Ok(value::BOXED_BINARY) => false,
        _ => return None, // TODO: BADARG
    };

    if writable {
        // TODO: we lookup twice, not good
        let sb = &mut binary.get_boxed_value_mut::<SubBinary>().unwrap();

        let pb = &mut sb.original;

        // OK, the binary is writable.

        let bin_size = 8 * sb.size + sb.bitsize;
        if unit > 1 {
            if (unit == 8 && (bin_size & 7) != 0) || (bin_size % unit) != 0 {
                return None; // TODO: BADARG
            }
        }

        if build_size_in_bits == 0 {
            // if (c_p->stop - c_p->htop < extra_words) {
            //     (void) erts_garbage_collect(c_p, extra_words, reg, live+1);
            //     bin = reg[live];
            // }
            return Some(binary);
        }

        // if (ERTS_UINT_MAX - build_size_in_bits) < bin_size {
        //     c_p->freason = SYSTEM_LIMIT;
        //     return THE_NON_VALUE;
        // }

        let used_size_in_bits = bin_size + build_size_in_bits as usize;

        sb.is_writable = false; /* Make sure that no one else can write. */
        let size = nbytes!(used_size_in_bits);
        // pb.flags |= PB_ACTIVE_WRITER;

        // Reserve extra capacity if needed.
        let data = pb.get_mut();
        if data.len() < size {
            data.reserve(2 * size); // why 2*?
        }
        process.context_mut().bs = Builder::new(pb);

        // Allocate heap space and build a new sub binary.

        // heap_need = ERL_SUB_BIN_SIZE + extra_words;
        // if (c_p->stop - c_p->htop < heap_need) {
        //     (void) erts_garbage_collect(c_p, heap_need, reg, live+1);
        // }

        Some(Term::subbinary(
            heap,
            SubBinary::new(pb.clone(), used_size_in_bits, 0, true),
        ))
    } else {
        // The binary is not writable. We must create a new writable binary and copy the old
        // contents of the binary.

        /*
         * Calculate sizes. The size of the new binary, is the sum of the
         * build size and the size of the old binary. Allow some room
         * for growing.
         */
        let (bin, bitoffs, bitsize) = match binary.get_boxed_header() {
            Ok(value::BOXED_BINARY) => {
                // TODO use ok_or to cast to some, then use ?
                let value = &binary.get_boxed_value::<RcBinary>().unwrap();
                (*value, 0, 0)
            }
            Ok(value::BOXED_SUBBINARY) => {
                // TODO use ok_or to cast to some, then use ?
                let value = &binary.get_boxed_value::<SubBinary>().unwrap();
                (&value.original, value.bit_offset, value.bitsize)
            }
            _ => unreachable!(),
        };
        let bin_size = 8 * binary_size!(binary) + bitsize;
        if unit > 1 {
            if (unit == 8 && (bin_size & 7) != 0) || (bin_size % unit) != 0 {
                return None; // TODO: BADARG
            }
        }

        if build_size_in_bits == 0 {
            return Some(binary);
        }

        //if (ERTS_UINT_MAX - build_size_in_bits) < bin_size {
        //    //p->freason = SYSTEM_LIMIT;
        //    return None;
        //}

        let used_size_in_bits = bin_size + build_size_in_bits as usize;
        let used_size_in_bytes = nbytes!(used_size_in_bits);

        let mut size = if used_size_in_bits < (std::u32::MAX as usize / 2) {
            2 * used_size_in_bytes
        } else {
            nbytes!(std::u32::MAX)
        };

        // Minimum off-heap capacity is 256 bytes.
        size = if size < 256 { 256 } else { size };

        // Allocate the binary data struct itself.
        // TODO: the RcBinary alloc needs to be sorted out so we don't leak
        let new_binary = heap.alloc(Arc::new(Binary::with_size(size))).clone();
        // ACTIVE_WRITER

        process.context_mut().bs = Builder::new(&new_binary);

        // Now copy the data into the binary.
        copy_binary!(
            new_binary.get_mut().as_mut_ptr(),
            0,
            bin.data.as_ptr(),
            bitoffs as usize,
            bin_size
        );

        // Now allocate the sub binary and set its size to include the data about to be built.
        Some(Term::subbinary(
            heap,
            SubBinary::new(new_binary, used_size_in_bits, 0, true),
        ))
    }
}

pub fn private_append(
    process: &RcProcess,
    binary: Term,
    build_size: Term,
    _unit: usize,
) -> Option<Term> {
    // Check and untag the requested build size.
    // if size < 0 || not smallint
    let build_size_in_bits = match build_size.into_variant() {
        value::Variant::Integer(i) if !i < 0 => i,
        // TODO: return err reason probs instead of tweaking freason
        // _ => return Err(Exception::new(Reason::EXC_BADARG)),
        _ => return None,
        // p->freason = BADARG;
    };

    let sb = &mut binary.get_boxed_value_mut::<SubBinary>().unwrap();
    assert!(sb.is_writable);

    let pb = &mut sb.original;

    // Calculate size in bytes.
    let bin_size = 8 * sb.size + sb.bitsize;

    let size_in_bits_after_build = bin_size + build_size_in_bits as usize;
    let size = (size_in_bits_after_build + 7) >> 3;
    // pb.flags |= PB_ACTIVE_WRITER; // TODO atomic set

    //if (ERTS_UINT_MAX - build_size_in_bits) < bin_size {
    //    //p->freason = SYSTEM_LIMIT;
    //    return None;
    //}

    // Reserve extra capacity if needed.
    let data = pb.get_mut();
    if data.capacity() < size {
        data.resize(2 * size, 0); // why 2*?
    }
    process.context_mut().bs = Builder::new(pb);

    sb.size = size_in_bits_after_build >> 3;
    sb.bitsize = size_in_bits_after_build & 7;
    Some(binary)
}

// TODO: transform into SubBinary::new() + is_writable
pub fn init_writable(process: &RcProcess, size: Term) -> Term {
    let size = match size.into_variant() {
        value::Variant::Integer(i) if i >= 0 => i as usize,
        _ => 1024,
    };

    let heap = &process.context_mut().heap;

    // TODO: the RcBinary alloc needs to be sorted out so we don't leak
    let binary = heap.alloc(Arc::new(Binary::with_size(size))).clone();

    Term::subbinary(
        heap,
        SubBinary::new(
            binary, // is_writable | active_writer
            0, 0, true,
        ),
    )
}

#[inline(always)]
fn get_bit(b: u8, offs: usize) -> u8 {
    (b >> (7 - offs)) & 1
}

/// Compare potentially unaligned bitstrings. Much slower than memcmp, so only use when necessary.
pub unsafe fn cmp_bits(
    mut a_ptr: *const u8,
    mut a_offs: usize,
    mut b_ptr: *const u8,
    mut b_offs: usize,
    mut size: usize,
) -> Ordering {
    // byte a;
    // byte b;
    // byte a_bit;
    // byte b_bit;
    // Uint lshift;
    // Uint rshift;
    // int cmp;

    assert!(a_offs < 8 && b_offs < 8);

    if size == 0 {
        return Ordering::Equal;
    }

    if ((a_offs | b_offs | size) & 7) == 0 {
        let byte_size = size >> 3;
        // compare as slices
        let slice_a = std::slice::from_raw_parts(a_ptr, byte_size);
        let slice_b = std::slice::from_raw_parts(b_ptr, byte_size);
        return slice_a.cmp(slice_b);
    }

    // Compare bit by bit until a_ptr is aligned on byte boundary
    let mut a = *a_ptr;
    a_ptr = a_ptr.offset(1);
    let mut b = *b_ptr;
    b_ptr = b_ptr.offset(1);
    if a_offs > 0 {
        loop {
            let a_bit: u8 = get_bit(a, a_offs);
            let b_bit: u8 = get_bit(b, b_offs);
            let cmp = a_bit.cmp(&b_bit);
            if cmp != Ordering::Equal {
                return cmp;
            }
            size -= 1;
            if size == 0 {
                return Ordering::Equal;
            }

            b_offs += 1;
            if b_offs == 8 {
                b_offs = 0;
                b = *b_ptr;
                b_ptr = b_ptr.offset(1);
            }
            a_offs += 1;
            if a_offs == 8 {
                a_offs = 0;
                a = *a_ptr;
                a_ptr = a_ptr.offset(1);
                break;
            }
        }
    }

    // Compare byte by byte as long as at least 8 bits remain
    if size >= 8 {
        let lshift = b_offs;
        let rshift = 8 - lshift;
        loop {
            let mut b_cmp: u8 = b << lshift;
            b = *b_ptr;
            b_ptr = b_ptr.offset(1);
            b_cmp |= b >> rshift;
            let cmp = a.cmp(&b_cmp);
            if cmp != Ordering::Equal {
                return cmp;
            }
            size -= 8;
            if size < 8 {
                break;
            }
            a = *a_ptr;
            a_ptr = a_ptr.offset(1);
        }

        if size == 0 {
            return Ordering::Equal;
        }
        a = *a_ptr;
        a_ptr = a_ptr.offset(1);
    }

    // Compare the remaining bits bit by bit
    if size > 0 {
        loop {
            let a_bit: u8 = get_bit(a, a_offs);
            let b_bit: u8 = get_bit(b, b_offs);
            let cmp = a_bit.cmp(&b_bit);
            if cmp != Ordering::Equal {
                return cmp;
            }
            size -= 1;
            if size == 0 {
                return Ordering::Equal;
            }

            a_offs += 1;
            assert!(a_offs < 8);

            b_offs += 1;
            if b_offs == 8 {
                b_offs = 0;
                b = *b_ptr;
                b_ptr = b_ptr.offset(1);
            }
        }
    }

    Ordering::Equal
}

/// The basic bit copy operation. Copies n bits from the source buffer to
/// the destination buffer. Depending on the directions, it can reverse the
/// copied bits.
pub unsafe fn copy_bits(
    mut src: *const u8, // Base pointer to source.
    soffs: usize,       // Bit offset for source relative to src.
    sdir: isize,        // Direction: 1 (forward) or -1 (backward).
    mut dst: *mut u8,   // Base pointer to destination.
    doffs: usize,       // Bit offset for destination relative to dst.
    ddir: isize,        // Direction: 1 (forward) or -1 (backward).
    n: usize,
) // Number of bits to copy.
{
    if n == 0 {
        return;
    }

    src = src.offset(sdir * byte_offset!(soffs) as isize);
    dst = dst.offset(ddir * byte_offset!(doffs) as isize);
    let soffs = bit_offset!(soffs);
    let doffs = bit_offset!(doffs);
    let deoffs = bit_offset!(doffs + n);
    let mut lmask = if doffs > 0 { make_mask!(8 - doffs) } else { 0 };
    let rmask = if deoffs > 0 {
        (make_mask!(deoffs)) << (8 - deoffs)
    } else {
        0
    };

    // Take care of the case that all bits are in the same byte.

    if doffs + n < 8 {
        // All bits are in the same byte
        lmask = if (lmask & rmask) > 0 {
            lmask & rmask
        } else {
            lmask | rmask
        };

        if soffs == doffs {
            *dst = mask_bits!(*src, *dst, lmask);
        } else if soffs > doffs {
            let mut bits: u8 = *src << (soffs - doffs); // TODO: is it u8
            if soffs + n > 8 {
                src = src.offset(sdir);
                bits |= *src >> (8 - (soffs - doffs));
            }
            *dst = mask_bits!(bits, *dst, lmask);
        } else {
            *dst = mask_bits!((*src >> (doffs - soffs)), *dst, lmask);
        }
        return; // We are done!
    }

    // At this point, we know that the bits are in 2 or more bytes.

    let mut count = (if lmask > 0 { n - (8 - doffs) } else { n }) >> 3;

    if soffs == doffs {
        /*
         * The bits are aligned in the same way. We can just copy the bytes
         * (except for the first and last bytes). Note that the directions
         * might be different, so we can't just use memcpy().
         */

        if lmask > 0 {
            *dst = mask_bits!(*src, *dst, lmask);
            dst = dst.offset(ddir);
            src = src.offset(sdir);
        }

        while count > 0 {
            count -= 1;
            *dst = *src;
            dst = dst.offset(ddir);
            src = src.offset(sdir);
        }

        if rmask > 0 {
            *dst = mask_bits!(*src, *dst, rmask);
        }
    } else {
        let mut bits: u8;
        let mut bits1: u8;
        let rshift;
        let lshift;

        // The tricky case. The bits must be shifted into position.

        if soffs > doffs {
            lshift = soffs - doffs;
            rshift = 8 - lshift;
            bits = *src;
            if soffs + n > 8 {
                src = src.offset(sdir);
            }
        } else {
            rshift = doffs - soffs;
            lshift = 8 - rshift;
            bits = 0;
        }

        if lmask > 0 {
            bits1 = bits << lshift;
            bits = *src;
            src = src.offset(sdir);
            bits1 |= bits >> rshift;
            *dst = mask_bits!(bits1, *dst, lmask);
            dst = dst.offset(ddir);
        }

        while count > 0 {
            count -= 1;
            bits1 = bits << lshift;
            bits = *src;
            src = src.offset(sdir);
            *dst = bits1 | (bits >> rshift);
            dst = dst.offset(ddir);
        }

        if rmask > 0 {
            bits1 = bits << lshift;
            if (rmask << rshift) > 0 {
                // (a << b) & 0xff but that is reduced anyway
                bits = *src;
                bits1 |= bits >> rshift;
            }
            *dst = mask_bits!(bits1, *dst, rmask);
        }
    }
}

#[derive(Debug)]
pub struct Builder {
    /// Pointer to the underlying binary storage
    binary: *mut Vec<u8>,
    /// Offset in bits
    pub offset: usize,
}

fn fmt_int(buf: &mut Vec<u8>, offset: usize, val: i32, bit_size: usize, flags: Flag) {
    let offs = bit_offset!(bit_size);
    let mut size = byte_offset!(bit_size);

    if offs > 0 {
        panic!("can't do bitaligned put yet");
    }

    let v = if flags.contains(Flag::BSF_LITTLE) {
        // Little endian
        // copy start to end, fixing up the last bit
        val.to_le_bytes()
    } else {
        /* Big endian */
        // copy end to start, fixing up the first bit?
        val.to_be_bytes()
    };
    let mut start = byte_offset!(offset);
    if size > v.len() {
        // pad it out if we have less bytes
        start = size - v.len();
        size = v.len();
    }
    // println!("start: {}, size: {}", start, size);
    buf[start..start + size].copy_from_slice(&v[..size]);
}

impl Builder {
    pub fn new(binary: &RcBinary) -> Self {
        Self {
            // :( we want to avoid locks so this method is for specifically when we know we're the only writer.
            binary: unsafe { &mut *(&binary.data as *const Vec<u8> as *mut Vec<u8>) },
            offset: 0,
        }
    }

    #[inline]
    pub fn data(&self) -> &mut Vec<u8> {
        unsafe { &mut *self.binary }
    }

    pub fn put_integer(&mut self, size: usize, mut flags: Flag, int: Term) {
        let bit_offset = bit_offset!(self.offset);

        // TODO: preprocess flags for native endian in loader(remove native_endian and set bsf_little off or on)
        native_endian!(flags);

        if let value::Variant::Integer(value) = int.into_variant() {
            match size {
                0 => (), // skip
                8 => self.data().push(value as u8),
                _ => {
                    let rbits = 8 - bit_offset;
                    if bit_offset + size <= 8 {
                        // All bits are in the same byte
                        // iptr = erts_current_bin + byte_offset!(self.offset);
                        // b = *iptr & (0xff << rbits);
                        // b |= (signed_val(arg) & ((1 << num_bits)-1)) << (8-bit_offset-num_bits);
                        // *iptr = b;
                        unimplemented!()
                    } else if bit_offset == 0 {
                        // More than one bit, starting at a byte boundary
                        let bits = bit_offset!(size);
                        fmt_int(self.data(), self.offset, value, size, flags);
                    } else if flags.contains(Flag::BSF_LITTLE) {
                        // special handling for little endian
                        unimplemented!()
                    } else {
                        // big endian
                        unimplemented!()
                    }
                }
            }
            self.offset += size;
        } else {
            panic!("Bad argument to put_integer")
        }
    }

    pub fn put_bytes(&mut self, binary: &[u8]) {
        let start = byte_offset!(self.offset);
        self.data()[start..start + binary.len()].copy_from_slice(&binary);
        self.offset += binary.len() * 8;
    }

    // pub fn put_binary(&mut self, binary: &[u8]) {
    //     let start = byte_offset!(self.offset);
    //     self.data()[start..start + binary.len()].copy_from_slice(&binary);
    //     self.offset += binary.len() * 8;
    // }

    pub fn put_binary_all(&mut self, binary: Term) {
        // convert binary to string
        let (bin, offs, bitoffs, size, bitsize) = match binary.get_boxed_header() {
            Ok(value::BOXED_BINARY) => {
                // TODO use ok_or to cast to some, then use ?
                let value = &binary.get_boxed_value::<RcBinary>().unwrap();
                (*value, 0, 0, value.data.len(), 0)
            }
            Ok(value::BOXED_SUBBINARY) => {
                // TODO use ok_or to cast to some, then use ?
                let value = &binary.get_boxed_value::<SubBinary>().unwrap();
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
        // TODO: all this math should be generalized
        // in bytes
        let total_bin_size = binary_size!(binary);
        let slice = &bin.data[offs..offs + total_bin_size];

        println!(
            "put_binary with slice: {:?}, tot: {}, whole: {:?}",
            slice, total_bin_size, bin.data
        );
        let start = byte_offset!(self.offset);
        self.data()[start..start + total_bin_size].copy_from_slice(&slice);
        self.offset += size * 8 + bitsize;
    }

    pub fn put_float(&mut self, float: f64) {
        let start = byte_offset!(self.offset);
        unsafe {
            let bytes: [u8; 8] = std::mem::transmute(float);
            self.data()[start..start + 8].copy_from_slice(&bytes);
        }
        self.offset += 8 * 8;
    }
}

pub fn bytes_to_list(
    heap: &Heap,
    mut previous: Term,
    bytes: &[u8],
    mut size: usize,
    bitoffs: u8,
) -> Term {
    if bitoffs == 0 {
        while size > 0 {
            size -= 1;
            previous = cons!(heap, Term::int(i32::from(bytes[size])), previous);
        }
    } else {
        let mut present: i32;
        let mut next: i32 = i32::from(bytes[size]);
        while size > 0 {
            present = next;
            size -= 1;
            next = i32::from(bytes[size]);
            previous = cons!(
                heap,
                Term::int(((present >> (8 - bitoffs)) | (next << bitoffs)) & 255),
                previous
            );
        }
    }
    previous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_integer() {
        // TODO start_match_2 should just be MatchBuffer::from(binary/subbinary)
        let binary = Arc::new(Binary::from(vec![0xAB, 0xCD, 0xEF]));

        let heap = Heap::new();

        let mut mb = MatchBuffer::from(binary);

        // fits in one byte
        let res = mb.get_integer(&heap, 4, Flag::BSF_NONE);
        assert_eq!(Some(Term::int(0xA)), res);

        // fits in two bytes
        let res = mb.get_integer(&heap, 8, Flag::BSF_NONE);
        assert_eq!(Some(Term::int(0xBC)), res);

        // num < small_bits & !little
        let res = mb.get_integer(&heap, 12, Flag::BSF_NONE);
        assert_eq!(Some(Term::int(0xDEF)), res);

        // TODO: signed, bigints
    }

    #[test]
    fn bitstring_unaligned() {
        let binary = Arc::new(Binary::from(vec![0x0B, 0xCD, 0xE]));
        let subbinary =
            SubBinary::new(Arc::new(Binary::from(vec![0xAB, 0xCD, 0xEF])), 20, 4, false);

        unsafe {
            assert!(
                cmp_bits(
                    binary.data.as_ptr(),
                    4,
                    subbinary.original.data.as_ptr(),
                    subbinary.bit_offset as usize,
                    subbinary.bitsize + subbinary.size
                ) == std::cmp::Ordering::Equal
            )
        }
    }

    #[test]
    fn bitstring_aligned() {
        let binary = Arc::new(Binary::from(vec![0xB, 0xCD, 0xE]));
        let subbinary = SubBinary::new(Arc::new(Binary::from(vec![0xB, 0xCD, 0xE])), 20, 0, false);

        unsafe {
            assert!(
                cmp_bits(
                    binary.data.as_ptr(),
                    0,
                    subbinary.original.data.as_ptr(),
                    subbinary.bit_offset as usize,
                    subbinary.bitsize + subbinary.size
                ) == std::cmp::Ordering::Equal
            )
        }
    }

    #[test]
    fn get_binary_all() {
        use crate::immix::Heap;
        let heap = Heap::new();
        let binary = Arc::new(Binary::from(vec![45, 114, 111, 111, 116]));
        let mut mb = MatchBuffer::from(binary);
        mb.offset += 8;
        let res = mb.get_binary_all(&heap, Flag::BSF_NONE).unwrap();

        let value = &res.get_boxed_value::<SubBinary>().unwrap();
        assert_eq!(4, value.size);
        assert_eq!(1, value.offset);

        assert_eq!(Some("root"), res.to_str());
    }

    #[test]
    fn get_bytes() {
        use crate::immix::Heap;
        let heap = Heap::new();
        let binary = Arc::new(Binary::from(vec![45, 114, 111, 111, 116]));
        let mut mb = MatchBuffer::from(binary);
        mb.offset += 8;
        let res = mb.get_bytes(2).unwrap();

        assert_eq!(&[114, 111], res.as_ref());
        assert_eq!(24, mb.offset);
    }

    #[test]
    fn test_builder() {
        let binary = Arc::new(Binary::from(vec![1, 2, 0, 0, 0, 0, 0, 0]));
        let mut builder = Builder::new(&binary);
        builder.offset = 16; // two bytes
        builder.put_integer(64, Flag::BSF_NONE, Term::int(257));
        assert_eq!(&[1, 2, 0, 0, 0, 0, 1, 1], builder.data().as_slice());
    }
}
