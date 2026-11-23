#include "side.h"
// The use is in a function only B has (WP-034: a body is digested, so a
// use added to a body A has changes that body).
// §11.4 FieldData `init` (landed WP-027): a field of an explicit instantiation
// definition is instantiated from the pattern; Kiln instantiates its default
// member initialiser on use, so it is absent and the use changes no unit.
template <class T> struct X { int m = 1; T t; void f() {} };
template struct X<int>;
#ifdef B
void use() { X<int> v; v.f(); }
#endif
