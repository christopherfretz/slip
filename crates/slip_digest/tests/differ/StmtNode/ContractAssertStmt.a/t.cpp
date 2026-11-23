// std: c++26
#include "side.h"
// §6 StmtNode `ContractAssertStmt.a`.
#ifdef B
void f(int x) { contract_assert(x > 1); }
#else
void f(int x) { contract_assert(x > 0); }
#endif
