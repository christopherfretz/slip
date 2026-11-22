#include "side.h"
// digest-kinds §9.5, WP-032 rule 3: a constrained non-template friend whose
// signature does not name `T` is keyed in each class specialisation that
// declares it, whichever comes first.
template <class T> struct A { friend int f(int) requires (sizeof(T) > 0) { return 1; } };
#ifdef B
A<long> b;
A<int> a;
#else
A<int> a;
A<long> b;
#endif
