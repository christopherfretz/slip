#include "side.h"
// The use is in a function only B has (WP-034: a body is digested, so a
// use added to a body A has changes that body).
// §11.4 FieldData `init` (landed WP-027): an implicit instantiation made
// before an explicit instantiation definition.
template <class T> struct X { int m = 1; T t; void f() {} };
X<int> v0;
template struct X<int>;
#ifdef B
void use() { X<int> v; v.f(); }
#endif
