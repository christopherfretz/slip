#include "side.h"
// §8 Scalar `Int.ty`: the same value of two enumerations.
template <auto V> struct A;
enum En1 { a1 };
enum En2 { a2 };
#ifdef B
A<En2(0)>* x;
#else
A<En1(0)>* x;
#endif
