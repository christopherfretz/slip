#include "side.h"
// §4.4 "Spellings", rule 10 (the review's z03b): R1: the found set of `f` in the qualifier of a resolved name in a template argument; clang 19: SFINAE differs (review recheck/clang/z03b.cpp).
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> A<Bq<sizeof(sizeof(f(t)))>::value>;
