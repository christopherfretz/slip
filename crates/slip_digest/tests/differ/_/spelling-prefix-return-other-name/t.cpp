#include "side.h"
// §4.4 "Spellings", rule 10 (the review's z03): R1: an instantiation-dependent operand in the qualifier of a resolved name in a template argument (`Bq<...>::value`), `f` against `h`.
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; };
int f(int); int h(int);
#ifdef B
template<class T> auto g(T t) -> A<Bq<sizeof(sizeof(h(t)))>::value>;
#else
template<class T> auto g(T t) -> A<Bq<sizeof(sizeof(f(t)))>::value>;
#endif
