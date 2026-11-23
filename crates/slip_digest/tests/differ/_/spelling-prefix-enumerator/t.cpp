#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y38): R1: an enumerator named through the qualifier.
template<int N> struct A {};
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t, A<Bq<sizeof(sizeof(h(T())))>::E1>*);
#else
template<class T> void g(T t, A<Bq<sizeof(sizeof(f(T())))>::E1>*);
#endif
