#include "side.h"
// §6 StmtKind `WhileStmt`: while against do (WP-034 extra test).
#ifdef B
void f(int x) { while (x) ; }
#else
void f(int x) { do ; while (x); }
#endif
