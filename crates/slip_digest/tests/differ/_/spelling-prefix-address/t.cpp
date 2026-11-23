#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y41): R1: the address of a static member function named through the qualifier.
template<auto P> struct Ap {};
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t, Ap<&Bq<sizeof(sizeof(h(T())))>::k>*);
#else
template<class T> void g(T t, Ap<&Bq<sizeof(sizeof(f(T())))>::k>*);
#endif
