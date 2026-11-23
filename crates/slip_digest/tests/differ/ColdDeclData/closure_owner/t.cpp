#include "side.h"
// §5 ColdDeclData `closure_owner`: a closure outside a function body is
// keyed under the declaration whose text holds its lambda-expression
// (§10.4). The variable that holds the lambda differs; `p`, whose type
// names the closure, changes with it (`g` and `h` are pending demand: an
// initialised variable's `eval`).
#ifdef B
auto g = [] { return 1; }; decltype(g) *p;
#else
auto h = [] { return 1; }; decltype(h) *p;
#endif
