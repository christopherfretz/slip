// std: c++26
#include "side.h"
// §6 ContractSite `comment-pre`: the predicate's spelling, of a precondition.
#ifdef B
void f(int x) pre( x > 0 ) {}
#else
void f(int x) pre(x>0) {}
#endif
