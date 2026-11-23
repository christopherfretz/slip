#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y39): R2: the full walk: in a default argument, `f` against `h`.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t, int = Bq<sizeof(sizeof(h(t)))>::value);
#else
template<class T> void g(T t, int = Bq<sizeof(sizeof(f(t)))>::value);
#endif
