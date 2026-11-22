#include "side.h"
// §8 Scalar `MemberPtr.path`: `L::x` as `int N::*` through `M1` against through `M2` (same type and member).
template <auto V> struct A;
struct L { int x; };
struct M1 : L {};
struct M2 : L {};
struct N : M1, M2 {};
#ifdef B
A<(int N::*)(int M2::*)&L::x>* x;
#else
A<(int N::*)(int M1::*)&L::x>* x;
#endif
