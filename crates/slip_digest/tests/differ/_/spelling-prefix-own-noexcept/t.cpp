#include "side.h"
// §4.4 "Spellings", rule 10 (the review's y42): R1: in an own `noexcept` operand.
template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
int f(int); int h(int);
#ifdef B
template<class T> void g(T t) noexcept(Bq<sizeof(sizeof(h(t)))>::value == 1);
#else
template<class T> void g(T t) noexcept(Bq<sizeof(sizeof(f(t)))>::value == 1);
#endif
