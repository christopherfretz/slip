#include "side.h"
// WP-035, digest-kinds §7 `_` `default-arg-block-scope-redeclaration`: the default comes from the block-scope redeclaration the call finds (kiln_sema expr.rs:6189); `CXXDefaultArgExpr.a` is the same, the redeclaration's default is not.
void f(int = 1);
void g() {
#ifdef B
  void f(int = 3);
#else
  void f(int = 2);
#endif
  f();
}
