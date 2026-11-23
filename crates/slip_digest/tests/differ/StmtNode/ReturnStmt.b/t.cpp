#include "side.h"
// §6 StmtNode `ReturnStmt.b`: the returned variable, which is also the copy-elision candidate (a unit test of stmt.rs holds the row alone).
struct S { S(); S(const S &); };
#ifdef B
S f(bool c) { S a; S b; if (c) return b; return a; }
#else
S f(bool c) { S a; S b; if (c) return a; return a; }
#endif
