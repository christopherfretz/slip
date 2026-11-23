#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y40): R2: the full walk: in a requires-clause.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> requires (Bq<sizeof(sizeof(h(T())))>::value == 1) void g(T);
#else
template<class T> requires (Bq<sizeof(sizeof(f(T())))>::value == 1) void g(T);
#endif
