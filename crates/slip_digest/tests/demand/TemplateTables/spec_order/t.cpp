#include "side.h"
// §9.5 TemplateTables `spec_order` (§11.8): the order in which
// specialisations were made is not in any unit.
template <class T> struct X { int a; };
int g() { X<int> a; return a.a; }
#ifdef B
int h() { X<long> b; X<int> a; return a.a + b.a; }
int k() { X<int> a; return a.a; }
#else
int h() { X<long> b; X<int> a; return a.a + b.a; }
int k() { return 0; }
#endif
