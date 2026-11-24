// std: c++26
#include "side.h"
// digest-kinds §3.3 (WP-066 review F2, Kiln WP-2545): a member `_` of an
// anonymous union is not name-independent ([basic.scope.scope]/5), so it
// names the union, and the union keeps its key when another member is
// renamed.
struct S {
#ifdef B
union { int _; int b; };
#else
union { int _; int a; };
#endif
};
