#include "side.h"
// §4.4 "Spellings": a found set in the qualifier of an elaborated type (a specialisation that is not dependent, rule 7).
template<int N> struct S { struct X {}; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, typename S<sizeof(sizeof(f(T())))>::X);
