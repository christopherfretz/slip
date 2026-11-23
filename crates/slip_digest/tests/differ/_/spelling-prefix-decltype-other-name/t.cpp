#include "side.h"
// §4.4 "Spellings", rule 10 (the review's z26): R1: in a `decltype` operand, `f` against `h`; clang 19: review recheck/clang/z26.cpp.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int); int h(int);
#ifdef B
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(h(t)))>::value);
#else
template<class T> auto g(T t) -> decltype(Bq<sizeof(sizeof(f(t)))>::value);
#endif
