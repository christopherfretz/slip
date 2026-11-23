#include "side.h"
// §4.4 "Spellings", rule 10 (the review's v4): R2: the full walk: the arguments of a variable template specialisation in a body (`vt<...>`); clang 19: review recheck/clang/v1v4.cpp.
template<int N> constexpr int vt = 1;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return vt<sizeof(sizeof(f(t)))>; }
