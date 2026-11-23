#include "side.h"
// §6 StmtNode `IndirectGotoStmt.a`.
#ifdef B
void f(void *p, void *q) { l: (void)&&l; goto *q; }
#else
void f(void *p, void *q) { l: (void)&&l; goto *p; }
#endif
