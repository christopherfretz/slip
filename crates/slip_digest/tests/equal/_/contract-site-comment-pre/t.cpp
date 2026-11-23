// std: c++26
#include "side.h"
// WP-034 (§6): the predicate's spelling, of a precondition (ContractSite).
#ifdef B
void f(int x) pre( x > 0 ) {}
#else
void f(int x) pre(x>0) {}
#endif
