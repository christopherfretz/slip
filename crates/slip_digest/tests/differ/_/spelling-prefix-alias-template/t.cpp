#include "side.h"
// §4.4 "Spellings", rule 10 (the review's z09): R1: the same through an alias template's target.
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> using Al = A<Bq<sizeof(sizeof(f(T())))>::value>;
