#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y37): R1: a call of a static member function named through the qualifier, in `decltype`.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(h(t)))>::k());
#else
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(f(t)))>::k());
#endif
