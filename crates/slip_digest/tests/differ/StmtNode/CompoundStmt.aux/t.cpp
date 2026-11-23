#include "side.h"
// §6 StmtNode `CompoundStmt.aux`: a pragma at the start of the block.
#ifdef B
void f() {
#pragma STDC FP_CONTRACT OFF
}
#else
void f() {
}
#endif
