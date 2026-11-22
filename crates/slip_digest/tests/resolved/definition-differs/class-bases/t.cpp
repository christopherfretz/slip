#include "side.h"
// digest-kinds §11.9 row `RecordData` `bases`.
struct B0 {};
struct B1 {};
#ifdef B
template <class T> struct X : B1 { int a; };
#else
template <class T> struct X : B0 { int a; };
#endif
X<int> x;
