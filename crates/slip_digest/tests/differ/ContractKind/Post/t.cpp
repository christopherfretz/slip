// std: c++26
#include "side.h"
// §6 ContractKind `Post`.
#ifdef B
void f(int x) post(x > 0) { }
#else
void f(int x) pre(x > 0) { }
#endif
