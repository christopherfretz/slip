#include "side.h"
// §6 StmtNode `LabelStmt.b`.
#ifdef B
void f() { l: return; }
#else
void f() { l: ; }
#endif
