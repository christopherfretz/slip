#include "side.h"
// §4.4 "Spellings", rule 7: an instantiation-dependent argument of a specialisation that is not dependent is a value in the canonical type; its profile and set are written from the spelling.
template<int N> struct A {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> A<sizeof(sizeof(f(t)))>;
