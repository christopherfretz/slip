#include "side.h"
// The use is in a function only B has (WP-034: a body is digested, so a
// use added to a body A has changes that body).
// §11.4 FieldData `init` (landed WP-027): the same under an explicit
// instantiation declaration (no call of f: X<int>::f() changing on use is
// demand/_/member-function-under-extern-template-use).
template <class T> struct X { int m = 1; T t; void f() {} };
extern template struct X<int>;
#ifdef B
void use() { X<int> v; (void)v; }
#endif
