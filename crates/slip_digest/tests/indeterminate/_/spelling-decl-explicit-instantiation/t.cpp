// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms (the audit of fix round 5): an explicit instantiation of a member: resolved where written. Side B adds `f(long)`.
// Moved from `differ` by WP-156 fix round 1: the qualifier's operand `f(1L)` is one Kiln does not keep as written, so the operand unit fails closed (`kiln/explicit-instantiation-written`, `spec/digest.md` §2.4 "What Kiln does not keep"); side A only.
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<int N> void Bq<N>::g() { }
template void Bq<sizeof(f(1L))>::g();
