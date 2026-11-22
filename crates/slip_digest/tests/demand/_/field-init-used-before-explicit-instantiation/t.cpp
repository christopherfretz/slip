#include "side.h"
// §11.4 FieldData `init` (landed WP-027): an implicit instantiation made
// before an explicit instantiation definition.
template <class T> struct X { int m = 1; T t; void f() {} };
X<int> v0;
template struct X<int>;
#ifdef B
void use() { X<int> v; v.f(); }
#else
void use() {}
#endif
