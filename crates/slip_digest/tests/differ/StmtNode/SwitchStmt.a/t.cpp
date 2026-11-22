#include "side.h"
// §6 StmtNode `SwitchStmt.a`.
#ifdef B
void f(int x) { switch (x + 1) {} }
#else
void f(int x) { switch (x) {} }
#endif
