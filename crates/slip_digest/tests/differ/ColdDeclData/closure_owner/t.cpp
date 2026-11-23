#include "side.h"
// §5 ColdDeclData `closure_owner`: a closure outside a function body is
// keyed under the declaration whose text holds its lambda-expression
// (§10.4). The variable that holds the lambda differs.
#ifdef B
auto g = [] { return 1; };
#else
auto h = [] { return 1; };
#endif
