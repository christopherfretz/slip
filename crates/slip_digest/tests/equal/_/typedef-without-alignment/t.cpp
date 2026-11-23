#include "side.h"
// digest-kinds §4.2 rule 9: a typedef carrying no alignment stays transparent (was the
// test of row `TypeView/Typedef` while the row was `out`; WP-107 made it `in`).
typedef int I;
#ifdef B
I v;
#else
int v;
#endif
