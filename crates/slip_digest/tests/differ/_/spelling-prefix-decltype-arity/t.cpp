#include "side.h"
// §4.4 "Spellings", rule 10 (the review's v3): R1: in a `decltype` operand, one argument against two.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int); int f(int, int);
#ifdef B
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(f(t, 0)))>::value);
#else
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(f(t)))>::value);
#endif
