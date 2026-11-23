// std: c++20
#include "side.h"
// §6 StmtKind `AttributedStmt`: `[[likely]]` (WP-034 extra test).
#ifdef B
void f(int x) { if (x) [[likely]] ; }
#else
void f(int x) { if (x) ; }
#endif
