#include "side.h"
// WP-034 (§6): braces around a single statement: Kiln builds a CompoundStmt for them, which has its own FP override and scope (landed WP-034).
#ifdef B
void f(int x) { if (x) { return; } }
#else
void f(int x) { if (x) return; }
#endif
