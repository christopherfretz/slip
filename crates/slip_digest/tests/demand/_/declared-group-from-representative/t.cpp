#include "side.h"
// digest §11, WP-032 rule 1: the instantiation in main declares `k` before
// its written declaration; the unit is in main's declared group on both
// sides, not in the header that holds the friend.
#include "a.h"
#ifdef B
void use() { A<int> a; }
#endif
void k(int);
