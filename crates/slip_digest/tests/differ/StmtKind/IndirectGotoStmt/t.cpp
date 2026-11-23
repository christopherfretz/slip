#include "side.h"
// §6 StmtKind `IndirectGotoStmt`.
#ifdef B
void f(void *p) { l: (void)&&l; goto *p; }
#else
void f(void *p) { l: (void)&&l; }
#endif
