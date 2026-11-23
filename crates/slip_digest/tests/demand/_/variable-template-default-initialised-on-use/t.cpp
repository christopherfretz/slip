#include "side.h"
// digest-kinds §11.9 (WP-045 review F3): v<S> is a definition from its
// declaration; Kiln adds its default-initialisation (a CXXConstructExpr)
// only on use (`kiln_sema/src/templ/var.rs:212-221`). The pattern has no
// initialiser, so `definition` does not write what Kiln adds.
struct S { S() {} int a; };
template <class T> T v;
using U = decltype(v<S>);
#ifdef B
int u() { return v<S>.a; }
#endif
