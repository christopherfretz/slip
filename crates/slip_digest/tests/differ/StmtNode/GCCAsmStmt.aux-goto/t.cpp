#include "side.h"
// §6 StmtNode `GCCAsmStmt.aux-goto`: `asm goto` has labels: the row is not alone here (a unit test of stmt.rs holds the bit).
#ifdef B
void f() { asm goto("" : : : : l); l: ; }
#else
void f() { asm("" : : : ); l: ; }
#endif
