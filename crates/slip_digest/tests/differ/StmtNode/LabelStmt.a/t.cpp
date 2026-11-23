#include "side.h"
// §6 StmtNode `LabelStmt.a`: the labels are written by ordinal: the one `goto` names is the second.
#ifdef B
void f() { m: ; l: ; goto l; }
#else
void f() { l: ; m: ; goto l; }
#endif
