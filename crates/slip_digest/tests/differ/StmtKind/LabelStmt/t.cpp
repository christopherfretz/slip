#include "side.h"
// §6 StmtKind `LabelStmt`.
#ifdef B
void f() { l: ; }
#else
void f() { ; }
#endif
