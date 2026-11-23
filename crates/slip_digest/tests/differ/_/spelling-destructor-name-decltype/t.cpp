#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): a destructor name `~decltype(e)` with an instantiation-dependent operand: side B adds `f(long)`, which the found set of `f` there does not hold.
template<int N> struct X {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, X<8> *q) { q->~decltype(X<sizeof(sizeof(f(t)))>{})(); }
