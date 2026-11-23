#include "side.h"
// §6 StmtKind `BreakStmt`: break against continue (WP-034 extra test).
#ifdef B
void f() { for (;;) { break; } }
#else
void f() { for (;;) { continue; } }
#endif
