// std: c++26
#include "side.h"
// §6 StmtNode `ContractAssertStmt.aux-kind`: an assertion in the body against a precondition.
#ifdef B
void f(int x) pre(x > 0) { }
#else
void f(int x) { contract_assert(x > 0); }
#endif
