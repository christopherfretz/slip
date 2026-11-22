#include "side.h"
// WP-034 (§6): a returned local whose class's implicit copy constructor B uses first.
struct S { int m; };
#ifdef B
S g(const S &a) { return a; }
#endif
S f() { S s; return s; }
