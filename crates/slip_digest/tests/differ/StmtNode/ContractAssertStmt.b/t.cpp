// std: c++26
#include "side.h"
// §6 StmtNode `ContractAssertStmt.b`: the result name of a postcondition.
#ifdef B
int f() post(r: true);
#else
int f() post(true);
#endif
