#include "side.h"
// §4.4 "Spellings", rule 10 (the review's z25): R1: the same in a base specifier.
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct D : A<Bq<sizeof(sizeof(f(T())))>::value> {};
