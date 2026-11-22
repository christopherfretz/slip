#include "side.h"
// std: c++26
// §6 ContractKind `Post`: against `pre`.
#ifdef B
void f(const int x) pre(x > 0);
#else
void f(const int x) post(x > 0);
#endif
