#include "side.h"
// §4.4 "Spellings", rule 10 (the review's t2): R1: in a partial specialisation's arguments.
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T, class U> struct C {};
template<class T> struct C<T, A<Bq<sizeof(sizeof(f(T())))>::value>> {};
