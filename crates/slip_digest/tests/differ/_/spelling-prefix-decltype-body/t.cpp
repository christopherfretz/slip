#include "side.h"
// §4.4 "Spellings", rule 11 (the re-check's k08): as spelling-prefix-decltype-body-added, `f(t)` against `h(t)`; clang 19: `g(X{})` is an error in A and compiles in B (review recheck/clang/k08.cpp).
template<int N> struct Bq { static constexpr int value = 1; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> int g(T t) { return decltype(Bq<sizeof(sizeof(h(t)))>{})::value; }
#else
template<class T> int g(T t) { return decltype(Bq<sizeof(sizeof(f(t)))>{})::value; }
#endif
