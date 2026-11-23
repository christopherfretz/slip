// std: c++26
#include "side.h"
// §6 StmtKind `ContractAssertStmt`.
#ifdef B
void f() { contract_assert(true); }
#else
void f() { ; }
#endif
