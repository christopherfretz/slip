#include "side.h"
// As differ/_/mem-initializer-in-template: a mem-initializer-id naming a base through an instantiation-dependent argument (rule 11).
template<int N> struct Bq { Bq(); Bq(int); };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S : Bq<8> { S() : Bq<sizeof(sizeof(f(T())))>() { } };
