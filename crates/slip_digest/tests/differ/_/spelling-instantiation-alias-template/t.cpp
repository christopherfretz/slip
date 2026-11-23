#include "side.h"
// §4.4 "Spellings", rule 7: as `spelling-instantiation-return-argument`, in an alias template.
template<int N> struct A {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> using Al = A<sizeof(sizeof(f(T())))>;
