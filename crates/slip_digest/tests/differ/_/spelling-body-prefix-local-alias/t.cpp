#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y43): R2: the full walk: in a block-scope alias.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t) { using R = decltype(Bq<sizeof(sizeof(h(t)))>::value); }
#else
template<class T> void g(T t) { using R = decltype(Bq<sizeof(sizeof(f(t)))>::value); }
#endif
