#include "side.h"
// digest §11, WP-032 rule 1: as `declared-group-from-representative`, with
// a written definition.
#include "a.h"
#ifdef B
void use() { A<int> a; }
#endif
void k(int) {}
