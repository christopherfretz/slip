#include "side.h"
// §9.5 TemplateTables `spec_order` (§11.8): side B makes `X<int>` before
// `X<long>`, side A after; the order is in no unit.
template <class T> struct X { int a; };
#ifdef B
X<int> a;
#else

#endif
X<long> b;
X<int> c;
