#include "side.h"
// §4.4 "Spellings", rule 1: a type requirement of a requires-expression read in a signature (a template argument), not in a constraint.
template<class> struct S {};
template<bool> struct W {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> W<requires { typename S<decltype(f(T()))>; }>;
