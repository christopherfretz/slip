#include "side.h"
// §6 StmtNode `CaseStmt.a`: the case value (WP-034 extra test).
#ifdef B
void f(int x) { switch (x) { case 2: ; } }
#else
void f(int x) { switch (x) { case 1: ; } }
#endif
