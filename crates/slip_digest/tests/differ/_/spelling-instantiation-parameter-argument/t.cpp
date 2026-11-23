#include "side.h"
// §4.4 "Spellings", rule 7: as `spelling-instantiation-return-argument`, in a parameter type.
template<int N> struct A {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, A<sizeof(sizeof(f(T())))> *);
