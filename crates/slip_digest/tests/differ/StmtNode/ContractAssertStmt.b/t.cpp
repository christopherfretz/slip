// std: c++26
#include "side.h"
// §6 StmtNode `ContractAssertStmt.b`: the result name of a postcondition.
#ifdef B
int f() post(r: r > 0);
#else
int f() post(r > 0);
#endif
