#include "side.h"
// §4.4 "Spellings", rule 10 (the review's v1): R2: the full walk: the found set of `f` in a function body; clang 19: B is ambiguous when instantiated (review recheck/clang/v1v4.cpp).
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return Bq<sizeof(sizeof(f(t)))>::value; }
