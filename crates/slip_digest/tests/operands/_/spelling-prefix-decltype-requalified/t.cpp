#include "side.h"
// §4.4 "Spellings", rule 10 (WP-156: the operand `x` is written in the operand units of g and h on side A alone, digest §2.4, S8): `decltype(x)::value` against `S::value`, `x` a non-dependent `S`: the qualifier as written is a `Decltype` whose operand names `x` and holds no found set, profile or alias, so the reading writes nothing, and the referent is the same: equal, in a template's body and in a function's.
struct S { static constexpr int value = 2; };
S x;
#ifdef B
template<class T> int g(T) { return S::value; }
int h() { return S::value; }
#else
template<class T> int g(T) { return decltype(x)::value; }
int h() { return decltype(x)::value; }
#endif
