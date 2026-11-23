#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y44): R1: in a dependent array bound.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t, char (*)[Bq<sizeof(sizeof(h(T())))>::value + 1]);
#else
template<class T> void g(T t, char (*)[Bq<sizeof(sizeof(f(T())))>::value + 1]);
#endif
