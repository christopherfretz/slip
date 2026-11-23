#include "side.h"
// §4.4 "Spellings": a found set in a type requirement.
template<class> struct S {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> requires requires { typename S<decltype(f(T()))>; } void g(T);
