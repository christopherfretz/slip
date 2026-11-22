#include "side.h"
// §10.1 AttrNode: `CLEANUP.a`: the node does not say whether `a` is a declaration or an expression.
void c(int *); void f() { int x __attribute__((cleanup(c))); }
