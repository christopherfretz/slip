#include "side.h"
// WP-156 fix round 3, Layer S: what the one traversal of a written type
// reads that the four recursions before it did not, each with an operand
// whose binding moves (`lib::make` against `::make<lib::X>`).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
// An instantiation-dependent array bound: its holders are read
// (`Slot::Bound`, mutation 3).
template <class T> struct PB { char a[sizeof(T) + sizeof(Wr<decltype(make(x))>)]; };
// A partial specialisation's own head (the census: `PartialSpecInfo`
// `params`).
template <int N, class T> struct PI {};
template <decltype(make(x)) N> struct PI<N, int> {};
