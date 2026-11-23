//! Floating-point state (`spec/digest-kinds.md` §6.1, WP-034): `record
//! FpOverride`, for `CompoundStmt.aux` and `ColdDeclData` `init_fp`.
//!
//! The enumerations of §6.1 (`FpContract`, `FpRound`, `FpEval`,
//! `FpExcept`) are written by their generated `row()`.
//!
//! An override is relative to the command line (Kiln fp.rs, "the fields
//! in `mask` take their value from `vals`, the others the command
//! line's"), and the command is not digested (digest §1 rule 8): the
//! record is written as Kiln stores it, `mask` included. A field of
//! `vals` whose `mask` bit is clear holds `FpOverride::NONE`'s value in
//! every override Kiln builds (`spec/landed/digest-kinds/WP-034.md`).

use slip_kiln::kiln::ast::fp::{FpOptions, FpOverride};

use crate::Cause;
use crate::stream::Stream;
use crate::tables::{fp_contract, fp_eval, fp_except, fp_round};

/// The bits of `mask` the rows of `FpOptions` account for: Kiln's
/// `fp_bit` constants, in the order of `FpOptions`' rows (`rounding_math`
/// has none: no pragma sets it). Held equal to Kiln's by a unit test.
pub(crate) const MASK_BITS: [(u16, &str); 12] = [
    (1, "contract"),
    (2, "reassoc"),
    (4, "arcp"),
    (8, "nnan"),
    (16, "ninf"),
    (32, "nsz"),
    (64, "afn"),
    (128, "math_errno"),
    (256, "round"),
    (512, "except"),
    (1024, "fenv_access"),
    (2048, "eval"),
];

/// A bit of `mask` no row of `FpOptions` accounts for.
const MASK_UNACCOUNTED: Cause = Cause::Unaccounted("FpOverride/mask");

/// `record FpOverride` (`CompoundStmt.aux`, `ColdDeclData` `init_fp`).
pub(crate) fn fp_override(s: &mut Stream, o: FpOverride) {
    let FpOverride { mask, vals, cx_limited, strict_fn } = o;
    // mask: uv.
    let known = MASK_BITS.iter().fold(0u16, |m, (b, _)| m | b);
    if mask & !known != 0 {
        s.indeterminate(MASK_UNACCOUNTED);
    }
    s.uv(u64::from(mask));
    // vals: record FpOptions.
    fp_options(s, vals);
    // cx_limited: bool?.
    s.opt(cx_limited.is_some());
    if let Some(on) = cx_limited {
        s.bool(on);
    }
    // strict_fn: bool.
    s.bool(strict_fn);
}

/// `record FpOptions`.
fn fp_options(s: &mut Stream, o: FpOptions) {
    let FpOptions {
        contract,
        reassoc,
        arcp,
        nnan,
        ninf,
        nsz,
        afn,
        math_errno,
        round,
        except,
        fenv_access,
        eval,
        rounding_math,
    } = o;
    s.tag(fp_contract::row(&contract));
    s.bool(reassoc).bool(arcp).bool(nnan).bool(ninf).bool(nsz).bool(afn).bool(math_errno);
    s.tag(fp_round::row(&round));
    s.opt(except.is_some());
    if let Some(e) = except {
        s.tag(fp_except::row(&e));
    }
    s.bool(fenv_access);
    s.tag(fp_eval::row(&eval));
    s.bool(rounding_math);
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::fp::{FpContract, FpEval, FpExcept, FpOverride, FpRound, fp_bit};

    use super::{MASK_BITS, fp_override};
    use crate::Cause;
    use crate::stream::Stream;

    fn bytes(o: FpOverride) -> Stream {
        let mut s = Stream::new();
        fp_override(&mut s, o);
        s
    }

    // §6.1 FpOverride `mask` (Open questions): the constants this file
    // relies on are Kiln's `fp_bit`, bit for bit, as `attrs.rs` holds
    // `KINDS`.
    #[test]
    fn mask_bits_are_kilns() {
        let kiln = [
            (fp_bit::CONTRACT, "contract"),
            (fp_bit::REASSOC, "reassoc"),
            (fp_bit::ARCP, "arcp"),
            (fp_bit::NNAN, "nnan"),
            (fp_bit::NINF, "ninf"),
            (fp_bit::NSZ, "nsz"),
            (fp_bit::AFN, "afn"),
            (fp_bit::ERRNO, "math_errno"),
            (fp_bit::ROUND, "round"),
            (fp_bit::EXCEPT, "except"),
            (fp_bit::FENV, "fenv_access"),
            (fp_bit::EVAL, "eval"),
        ];
        assert_eq!(MASK_BITS, kiln);
        // `set_flag` and the other setters set exactly the bit named.
        let mut o = FpOverride::NONE;
        o.set_round(FpRound::Upward);
        assert_eq!(o.mask, fp_bit::ROUND);
    }

    // §6.1 FpOverride `mask`: a bit no row accounts for is unaccounted,
    // the stream well formed.
    #[test]
    fn an_unknown_mask_bit_is_unaccounted() {
        let mut o = FpOverride::NONE;
        o.mask = 1 << 12;
        let s = bytes(o);
        assert_eq!(s.cause(), Some(Cause::Unaccounted("FpOverride/mask")));
        assert_eq!(s.as_bytes().len(), bytes(FpOverride::NONE).as_bytes().len() + 1);
        let mut o = FpOverride::NONE;
        o.mask = 0xFFF;
        assert_eq!(bytes(o).cause(), None);
    }

    /// Every row of `FpOverride` and `FpOptions`, changed one at a time
    /// from `NONE` (and the enumerations to each of their variants), gives
    /// bytes of its own: no row is skipped or written as a constant
    /// (WP-034 "Rules"; each row's pair test shows it from source where a
    /// pragma reaches it alone).
    #[test]
    fn every_row_is_written() {
        let base = FpOverride::NONE;
        let mut cases: Vec<(&str, FpOverride)> = Vec::new();
        let mut with = |name: &'static str, f: &dyn Fn(&mut FpOverride)| {
            let mut o = base;
            f(&mut o);
            cases.push((name, o));
        };
        with("mask", &|o| o.mask = fp_bit::NSZ);
        with("cx_limited on", &|o| o.cx_limited = Some(true));
        with("cx_limited off", &|o| o.cx_limited = Some(false));
        with("strict_fn", &|o| o.strict_fn = true);
        with("contract off", &|o| o.vals.contract = FpContract::Off);
        with("contract fast", &|o| o.vals.contract = FpContract::Fast);
        with("reassoc", &|o| o.vals.reassoc = true);
        with("arcp", &|o| o.vals.arcp = true);
        with("nnan", &|o| o.vals.nnan = true);
        with("ninf", &|o| o.vals.ninf = true);
        with("nsz", &|o| o.vals.nsz = true);
        with("afn", &|o| o.vals.afn = true);
        with("math_errno", &|o| o.vals.math_errno = true);
        with("round nearest", &|o| o.vals.round = FpRound::Nearest);
        with("round upward", &|o| o.vals.round = FpRound::Upward);
        with("round downward", &|o| o.vals.round = FpRound::Downward);
        with("round toward zero", &|o| o.vals.round = FpRound::TowardZero);
        with("round nearest away", &|o| o.vals.round = FpRound::NearestAway);
        with("except ignore", &|o| o.vals.except = Some(FpExcept::Ignore));
        with("except maytrap", &|o| o.vals.except = Some(FpExcept::MayTrap));
        with("except strict", &|o| o.vals.except = Some(FpExcept::Strict));
        with("fenv_access", &|o| o.vals.fenv_access = true);
        with("eval double", &|o| o.vals.eval = FpEval::Double);
        with("eval extended", &|o| o.vals.eval = FpEval::Extended);
        with("rounding_math", &|o| o.vals.rounding_math = true);
        let mut seen = vec![("NONE", bytes(base).as_bytes().to_vec())];
        for (name, o) in cases {
            let b = bytes(o).as_bytes().to_vec();
            if let Some((other, _)) = seen.iter().find(|(_, x)| *x == b) {
                panic!("{name} writes the bytes of {other}");
            }
            seen.push((name, b));
        }
    }

    // §6.1: the stream of the empty override, row by row.
    #[test]
    fn the_empty_override() {
        let mut want = Stream::new();
        want.uv(0);
        want.tag(crate::tables::fp_contract::ON);
        for _ in 0..7 {
            want.bool(false);
        }
        want.tag(crate::tables::fp_round::DYNAMIC);
        want.opt(false);
        want.bool(false);
        want.tag(crate::tables::fp_eval::SOURCE);
        want.bool(false);
        want.opt(false);
        want.bool(false);
        assert_eq!(bytes(FpOverride::NONE).as_bytes(), want.as_bytes());
    }
}
