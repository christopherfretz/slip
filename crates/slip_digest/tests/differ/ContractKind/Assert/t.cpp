#include "side.h"
// std: c++26
// §6 ContractKind `Assert`: against `[[assume]]`.
#ifdef B
void f(int x) { [[assume(x > 0)]]; }
#else
void f(int x) { contract_assert(x > 0); }
#endif
