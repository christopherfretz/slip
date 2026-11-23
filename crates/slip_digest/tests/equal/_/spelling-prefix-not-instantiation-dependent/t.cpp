#include "side.h"
// §4.4 "Spellings", rule 10: an argument of a prefix that is not instantiation-dependent is its value: `Bq<sizeof(int)>::value` against `Bq<4>::value`, in a body and in a signature.
template<int N> struct Bq { static constexpr int value = 1; };
template<int N> struct A {};
#ifdef B
template<class T> int g(T) { return Bq<4>::value; }
template<class T> auto k(T t) -> A<Bq<4>::value>;
#else
template<class T> int g(T) { return Bq<sizeof(int)>::value; }
template<class T> auto k(T t) -> A<Bq<sizeof(int)>::value>;
#endif
