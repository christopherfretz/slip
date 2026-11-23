#include "side.h"
// §6 StmtNode `GotoStmt.a`.
#ifdef B
void f() { l: ; m: ; goto m; }
#else
void f() { l: ; m: ; goto l; }
#endif
