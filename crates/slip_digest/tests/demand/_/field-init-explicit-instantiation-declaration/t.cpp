#include "side.h"
// §11.4 FieldData `init` (landed WP-027): the same under an explicit
// instantiation declaration (no call of f: X<int>::f() changing on use is
// demand/_/member-function-under-extern-template-use).
template <class T> struct X { int m = 1; T t; void f() {} };
extern template struct X<int>;
#ifdef B
void use() { X<int> v; (void)v; }
#else
void use() {}
#endif
