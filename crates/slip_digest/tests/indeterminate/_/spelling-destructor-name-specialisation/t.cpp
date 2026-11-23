#include "side.h"
// §4.4 "Spellings", rule 11, audit (c) of fix round 4: a destructor named by member access as `~X<..>` with an instantiation-dependent argument: Kiln resolves it to `X<8>::~X` and keeps nothing of the written name, so the unit is indeterminate (`spelling/destructor-name`) rather than equal (side B adds `f(long)`, which the found set of `f` there does not hold).
template<int N> struct X {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, X<8> *q) { q->~X<sizeof(sizeof(f(t)))>(); }
