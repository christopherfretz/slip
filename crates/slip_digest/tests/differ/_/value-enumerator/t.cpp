#include "side.h"
// §8 Scalar `Int`, digest §7: two enumerators of one enumeration.
template <auto V> struct A;
enum En { e1, e2 };
#ifdef B
A<e2>* x;
#else
A<e1>* x;
#endif
