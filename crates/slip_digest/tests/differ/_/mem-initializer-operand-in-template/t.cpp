#include "side.h"
// As differ/_/mem-initializer-in-template: an operand whose found set side B changes.
template<int N> struct Bq { Bq(); Bq(int); };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { int m; S() : m(sizeof(f(T()))) { } };
