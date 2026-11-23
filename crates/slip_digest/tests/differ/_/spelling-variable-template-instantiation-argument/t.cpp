#include "side.h"
// §4.4 "Spellings", rule 1: the written template arguments of a resolved reference to a variable template specialisation (a `DeclRefExpr`), instantiation-dependent and not dependent.
template<class U> constexpr int v = 0;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(v<decltype(sizeof(f(T())))> + t);
